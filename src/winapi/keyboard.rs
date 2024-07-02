//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::{mem, ptr};

use encode_unicode::CharExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Use invalid pointers to track state.
const HOOK_INVALID: usize = 0;
const HOOK_EXECUTING: usize = 1;

thread_local! {
    /// Stores a type erased pointer to the hook closure.
    static HOOK: Cell<usize> = const { Cell::new(HOOK_INVALID) };
    static QUEUED_INPUTS: RefCell<Vec<INPUT>> = const { RefCell::new(Vec::new()) };
}

/// Wrapper for the low-level keyboard hook API.
/// Automatically unregisters the hook when dropped.
pub struct KeyboardHook<F> {
    handle: HHOOK,
    _hook_proc: Box<F>,
}

impl<F> KeyboardHook<F>
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// The closure receives key press and key release events. When the closure
    /// returns `false` the key event is not modified and forwarded as if
    /// nothing happened. To ignore a key event or to remap it to another
    /// key return `true` and use [`send_key()`].
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediately be unregistered and not work."]
    pub fn set(callback: F) -> Self {
        assert!(
            HOOK.get() == 0,
            "Only one keyboard hook can be registered per thread."
        );

        let mut callback = Box::new(callback);
        HOOK.set(&mut *callback as *mut F as usize);

        let handle = unsafe { SetWindowsHookExA(WH_KEYBOARD_LL, Some(hook_proc::<F>), 0, 0) };
        assert_ne!(handle, 0, "Failed to install low-level keyboard hook.");
        KeyboardHook {
            handle,
            _hook_proc: callback,
        }
    }
}

impl<F> Drop for KeyboardHook<F> {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
        HOOK.set(HOOK_INVALID);
    }
}

/// Type of a key event.
#[derive(Debug, Clone, Copy)]
pub enum KeyType {
    /// Virtual key as defined by the layout set by windows.
    ///
    /// <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    VirtualKey(u8),

    /// Unicode character.
    Unicode(char),
}

/// Key event received by the low level keyboard hook.
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// Virtual key or unicode character of this event.
    pub key: KeyType,

    /// Scan code as defined by the keyboard.
    /// Extended keycodes have the three most significant bits set (0xExxx).
    pub scan_code: u16,

    /// Key was released
    pub up: bool,

    /// Time in milliseconds since boot.
    pub time: u32,
}

impl Display for KeyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{{ sc: {:#06X}, ", self.scan_code))?;

        match self.key {
            KeyType::VirtualKey(vk) => f.write_fmt(format_args!("vk: {:#04X}", vk))?,
            KeyType::Unicode(c) => f.write_fmt(format_args!("char: {}", c))?,
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

/// The actual WinAPI compatible callback.
unsafe extern "system" fn hook_proc<F>(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    if code != HC_ACTION as i32 {
        return CallNextHookEx(0, code, wparam, lparam);
    }

    let hook_lparam = &*(lparam as *const KBDLLHOOKSTRUCT);
    let key_event = KeyEvent::from_hook_lparam(hook_lparam);
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;

    // `SendInput()` internally calls the hook function. Filter out injected
    // events to prevent recursion and potential stack overflows if our
    // remapping logic has sent the injected event.
    if injected {
        return CallNextHookEx(0, code, wparam, lparam);
    }

    // There are two conditions for which the hook can be re-entered:
    // 1. `SendInput()` creates new injected input events.
    //   We queue input events and send them after the hook closure has
    //   returned to prevent a second mutable borrow to the closure.
    // 2. `CallNextHookEx()` when more than one unhandled input event is queued.
    //   Not exposed to the user and such must not be called within the closure.
    //
    // How to trigger 2.:
    // The classic CMD window has a "Quick Edit Mode" option which is enabled
    // by default. Windows stops to read from stdout and stderr when the user
    // selects characters in the CMD window.
    // Any write to stdout (e.g. a call to `println!()`) blocks while
    // "Quick Edit Mode" is active. The key event which exits the "Quick Edit
    // Mode" triggers the hook a second time.

    // Replace the pointer to the closure with a marker, so that `send_key()`
    // can detect if it was called from within the hook.
    let hook_ptr = HOOK.replace(HOOK_EXECUTING) as *mut F;
    let hook = unsafe { hook_ptr.as_mut().unwrap() };
    let handled = hook(key_event);
    HOOK.set(hook_ptr as usize);

    send_queued_inputs();

    if handled {
        -1
    } else {
        CallNextHookEx(0, code, wparam, lparam)
    }
}

/// Sends a virtual key event.
pub fn send_key(key: KeyEvent) {
    QUEUED_INPUTS.with_borrow_mut(|queued_inputs| {
        match key.key {
            KeyType::VirtualKey(vk) => {
                queued_inputs.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: vk.into(),
                            wScan: key.scan_code,
                            dwFlags: if key.up { KEYEVENTF_KEYUP } else { 0 },
                            time: key.time,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
            KeyType::Unicode(c) => {
                // Sends a unicode character, knows as `VK_PACKET`.
                // Interestingly this is faster than sending a regular virtual key event.
                for c in c.to_utf16() {
                    queued_inputs.push(INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: 0,
                                wScan: c,
                                dwFlags: KEYEVENTF_UNICODE
                                    | if key.up { KEYEVENTF_KEYUP } else { 0 },
                                time: key.time,
                                dwExtraInfo: 0,
                            },
                        },
                    });
                }
            }
        };
    });

    if HOOK.get() != HOOK_EXECUTING {
        // Send inputs only when not called from within the hook process.
        send_queued_inputs();
    } else {
        // Events will be sent when leaving the hook to prevent re-entrancy edge cases.
    }
}

fn send_queued_inputs() {
    let mut queued_inputs = QUEUED_INPUTS.with_borrow_mut(std::mem::take);

    if !queued_inputs.is_empty() {
        unsafe {
            SendInput(
                queued_inputs.len() as u32,
                queued_inputs.as_ptr(),
                mem::size_of::<INPUT>() as _,
            )
        };
        queued_inputs.clear();
    }

    // Re-use the previous allocation
    QUEUED_INPUTS.with_borrow_mut(move |qi| std::mem::replace(qi, queued_inputs));
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
        if layout == 0 {
            layout = GetKeyboardLayout(0);
        }
        let vk_state = VkKeyScanExW(c.to_utf16()[0], layout);
        if vk_state == -1 {
            // No virtual key for this character exists on the current layout.
            return None;
        }

        let dead_key =
            MapVirtualKeyExW((vk_state & 0xFF) as u32, MAPVK_VK_TO_CHAR, layout) & 0x80000000 != 0;
        if dead_key {
            // We have virtual key but it is a dead-key, e.g.: `^` or `~` on international layouts.
            return None;
        }

        // Check if the modifier keys, which are required to type the character, are pressed.
        let modifier_pressed = |vk: u16| (GetKeyState(vk.into()) as u16) & 0x8000 != 0;

        let shift = vk_state & 0x100 != 0;
        if shift && (modifier_pressed(VK_SHIFT) == caps_lock_enabled()) {
            // Shift required but not pressed and caps lock disabled OR
            // Shift required and pressed but caps lock enabled (which cancels the shift press).
            return None;
        }

        let ctrl = vk_state & 0x200 != 0;
        if ctrl && !modifier_pressed(VK_CONTROL) {
            return None;
        }

        let alt = vk_state & 0x400 != 0;
        if alt && !modifier_pressed(VK_CONTROL) {
            return None;
        }

        Some(vk_state as u8)
    }
}

pub fn caps_lock_enabled() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL.into()) as u16) & 0x0001 != 0 }
}
