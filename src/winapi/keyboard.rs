//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt::Display;
use std::future::poll_fn;
use std::task::{Poll, Waker};
use std::{mem, ptr};

use encode_unicode::CharExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    /// Buffer key events to prevent blocking the low-level keyboard hook.
    static KEY_QUEUE: RefCell<KeyQueue> = const { RefCell::new(KeyQueue::new()) };
    static HOOK_HANDLE: Cell<HHOOK> = const { Cell::new(ptr::null_mut()) };
}

/// Installs the low-level keyboard hook for this thread.
/// Warning: Captures all keyboard events system-wide, no other application will
/// receive keyboard events until the hook is removed by calling `hook_disable()`.
pub fn hook_enable() {
    if HOOK_HANDLE.get().is_null() {
        let handle =
            unsafe { SetWindowsHookExA(WH_KEYBOARD_LL, Some(hook_proc), ptr::null_mut(), 0) };
        assert!(
            !handle.is_null(),
            "Failed to install low-level keyboard hook."
        );
        HOOK_HANDLE.set(handle);
    }
}

/// Removes the low-level keyboard hook for this thread.
pub fn hook_disable() {
    let handle = HOOK_HANDLE.get();
    if !handle.is_null() {
        unsafe {
            UnhookWindowsHookEx(handle);
        }
        HOOK_HANDLE.set(ptr::null_mut());
    }
}

/// Asynchronously waits for the next key event captured by the low-level keyboard hook.
pub async fn next_key_event() -> KeyEvent {
    poll_fn(|cx| {
        KEY_QUEUE.with_borrow_mut(|queue| {
            if let Some(key) = queue.key_events.pop_front() {
                Poll::Ready(key)
            } else {
                queue.waker.push(cx.waker().clone());
                Poll::Pending
            }
        })
    })
    .await
}

/// Key event type.
#[derive(Debug, Clone, Copy)]
pub enum KeyType {
    /// Virtual key as defined by the layout set by Windows.
    ///
    /// <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    VirtualKey(u8),

    /// Unicode character.
    Unicode(char),
}

/// Key event received by the low-level keyboard hook.
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// Virtual key or Unicode character of this event.
    pub key: KeyType,

    /// Scan code as defined by the keyboard.
    /// Extended keycodes have the three most significant bits set (0xExxx).
    pub scan_code: u16,

    /// Key was released.
    pub up: bool,

    /// Time in milliseconds since boot.
    pub time: u32,
}

impl Display for KeyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{{ sc: {:#06X}, ", self.scan_code))?;

        match self.key {
            KeyType::VirtualKey(vk) => f.write_fmt(format_args!("vk: {vk:#04X}"))?,
            KeyType::Unicode(c) => f.write_fmt(format_args!("char: {c}"))?,
        }

        f.write_fmt(format_args!(
            ", {} }}",
            if self.up { "up  " } else { "down" }
        ))?;

        Ok(())
    }
}

impl KeyEvent {
    fn from_hook_lparam(lparam: &KBDLLHOOKSTRUCT) -> Self {
        let mut scan_code = lparam.scanCode as u16;
        if lparam.flags & LLKHF_EXTENDED != 0 {
            scan_code |= 0xE000;
        }

        Self {
            key: KeyType::VirtualKey(lparam.vkCode as _),
            scan_code,
            up: lparam.flags & LLKHF_UP != 0,
            time: lparam.time,
        }
    }
}

/// The actual WinAPI compatible hook callback function.
/// Called from `KiUserCallbackDispatcher()` context as described in
/// (this blog post)[http://www.nynaeve.net/?p=204]. Might be re-entered.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
    }

    let hook_lparam = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;

    // `SendInput()` internally triggers the hook function. Filter out injected
    // events to prevent an infinite loop if our remapping logic has sent the
    // injected event.
    if injected {
        return unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) };
    }

    // Windows re-enters the hook function for two conditions:
    // 1. `SendInput()` called from within the hook, which produces an injected
    //    key. Additionally to the injected key, all other regular key events
    //    are passed to the hook before `SendInput()` returns.
    // 2. The hook blocks longer than the number of ms specified in the registry
    //    key `HKEY_CURRENT_USER\Control Panel\LowLevelHooksTimeout`.
    // Our solution to both scenarios is to buffer all key events and process
    // them asynchronously outside of the hook context.
    // This has the downside that we catch every key event and downstream
    // keyboards hooks only ever see injected events. So far this did not cause
    // any issues in practice.

    let key = KeyEvent::from_hook_lparam(hook_lparam);
    KEY_QUEUE.with(|queue| queue.borrow_mut().enqueue(key));
    -1
}

struct KeyQueue {
    key_events: VecDeque<KeyEvent>,
    waker: Vec<Waker>,
}

impl KeyQueue {
    const fn new() -> Self {
        Self {
            key_events: VecDeque::new(),
            waker: Vec::new(),
        }
    }

    fn enqueue(&mut self, key: KeyEvent) {
        self.key_events.push_back(key);
        for waker in self.waker.drain(..) {
            waker.wake();
        }
    }
}

/// Sends a virtual key event.
pub fn send_key(key: KeyEvent) {
    unsafe {
        let mut inputs: [INPUT; 2] = mem::zeroed();

        let n_inputs = match key.key {
            KeyType::VirtualKey(vk) => {
                inputs[0].r#type = INPUT_KEYBOARD;
                inputs[0].Anonymous.ki = KEYBDINPUT {
                    wVk: vk.into(),
                    wScan: key.scan_code,
                    dwFlags: if key.up { KEYEVENTF_KEYUP } else { 0 },
                    time: key.time,
                    dwExtraInfo: 0,
                };
                1
            }
            KeyType::Unicode(c) => {
                // Sends a unicode character, knows as `VK_PACKET`.
                // Interestingly this is faster than sending a regular virtual key event.
                inputs
                    .iter_mut()
                    .zip(c.to_utf16())
                    .map(|(input, c)| {
                        input.r#type = INPUT_KEYBOARD;
                        input.Anonymous.ki = KEYBDINPUT {
                            wVk: 0,
                            wScan: c,
                            dwFlags: KEYEVENTF_UNICODE | if key.up { KEYEVENTF_KEYUP } else { 0 },
                            time: key.time,
                            dwExtraInfo: 0,
                        };
                    })
                    .count()
            }
        };

        SendInput(
            n_inputs as _,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as _,
        );
    }
}

/// Returns a virtual key code if the requested character can be typed with a
/// single key press/release.
pub fn get_virtual_key(c: char) -> Option<u8> {
    unsafe {
        let mut layout = GetKeyboardLayout(GetWindowThreadProcessId(
            GetForegroundWindow(),
            ptr::null_mut(),
        ));
        // For console applications (e.g. cmd.exe) GetKeyboardLayout() will
        // return 0. Windows has no public API to get the current layout of
        // console applications: https://github.com/microsoft/terminal/issues/83
        // Fall back to the layout used by our process which is hopefully the
        // correct one for the console too.
        if layout.is_null() {
            layout = GetKeyboardLayout(0);
        }
        let vk_state = VkKeyScanExW(c.to_utf16()[0], layout);
        if vk_state == -1 {
            // No virtual key for this character exists on the current layout.
            return None;
        }

        let crtl_or_alt_required = vk_state & 0x600 != 0;
        if crtl_or_alt_required {
            return None;
        }

        let dead_key =
            MapVirtualKeyExW((vk_state & 0xFF) as u32, MAPVK_VK_TO_CHAR, layout) & 0x80000000 != 0;
        if dead_key {
            // We have a virtual key but it is a dead-key, e.g.: `^` or `~` on international layouts.
            return None;
        }

        let shift_required = vk_state & 0x100 != 0;

        // The shift state is canceled when shift pressed but caps lock is enabled.
        let shift_pressed = GetAsyncKeyState(VK_SHIFT.into()) < 0;
        let shift_state = shift_pressed != caps_lock_enabled();

        if shift_required != shift_state {
            return None;
        }

        Some(vk_state as u8)
    }
}

pub fn caps_lock_enabled() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL.into()) as u16) & 0x0001 != 0 }
}
