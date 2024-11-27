//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::marker::PhantomData;
use std::{mem, ptr};

use encode_unicode::CharExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Use invalid pointers to track state.
const HOOK_INVALID: usize = 0;
const HOOK_EXECUTING: usize = 1;

thread_local! {
    /// Stores a type-erased pointer to the hook closure.
    static HOOK: Cell<usize> = const { Cell::new(HOOK_INVALID) };
    static QUEUED_INPUTS: RefCell<Vec<INPUT>> = const { RefCell::new(Vec::new()) };
}

/// Wrapper for the low-level keyboard hook API.
/// Automatically unregisters the hook when dropped.
pub struct KeyboardHook<F> {
    handle: HHOOK,
    // Required for drop to properly drop the closure.
    _closure_type: PhantomData<F>,
}

impl<F> KeyboardHook<F>
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// The closure receives key press and key release events. When the closure
    /// returns `false`, the key event is not modified and forwarded as if
    /// nothing happened. To ignore a key event or to remap it to another
    /// key, return `true` and use [`send_key()`].
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediately be unregistered and not work."]
    pub fn set(callback: F) -> Self {
        assert!(
            HOOK.get() == 0,
            "Only one keyboard hook can be registered per thread."
        );

        let callback = Box::into_raw(Box::new(callback));
        HOOK.set(callback as usize);

        let handle =
            unsafe { SetWindowsHookExA(WH_KEYBOARD_LL, Some(hook_proc::<F>), ptr::null_mut(), 0) };
        assert!(
            !handle.is_null(),
            "Failed to install low-level keyboard hook."
        );
        KeyboardHook {
            handle,
            _closure_type: PhantomData,
        }
    }
}

impl<F> Drop for KeyboardHook<F> {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.handle);
            drop(Box::from_raw(HOOK.replace(HOOK_INVALID) as *mut F));
        }
    }
}

/// Type of a key event.
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

/// The actual WinAPI compatible hook callback function.
/// Called from `KiUserCallbackDispatcher()` context as described in
/// (this blog post)[http://www.nynaeve.net/?p=204]. Might be re-entered.
unsafe extern "system" fn hook_proc<F>(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    if code != HC_ACTION as i32 {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let hook_lparam = &*(lparam as *const KBDLLHOOKSTRUCT);
    let key_event = KeyEvent::from_hook_lparam(hook_lparam);
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;

    // `SendInput()` internally calls the hook function. Filter out injected
    // events to prevent recursion and potential stack overflows if our
    // remapping logic has sent the injected event.
    if injected {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    // There is at least one edge case where this hook function is re-entered and
    // we reach here because we received a real event while either blocking on
    // `CallNextHookEx()` or while executing the user-provided hook closure.
    // The latter being problematic because we cannot have more than one mutable
    // reference to the closure at a time.
    //
    // To reproduce how the issue was discovered:
    // 1. Compile a version of this code with a `println!()` call at the
    //    beginning of the hook closure.
    // 2. Run the compiled binary twice (but not from a command line window).
    // 3. In the instance launched first (order is important), open the debug
    //    console and enable "QuickEdit Mode" in the console properties by
    //    right-clicking the title bar.
    // 4. Select some text in the console window of the first instance. Windows
    //    now blocks the process running in the console when accessing stdout.
    // 5. Now type a key that is remapped in the config.
    // 6. The second instance will crash.
    //
    // It appears that the hook function will only be re-entered by new real
    // keyboard events when it blocks longer than the number of ms specified in
    // the registry key `HKEY_CURRENT_USER\Control Panel\LowLevelHooksTimeout`.
    //
    // That is why the user-provided hook closure must not block. Unfortunately,
    // calls to the `SendInput()` function may block if another low-level
    // keyboard hook is registered (in a different thread) after ours in the
    // hook chain. As remapping keys is one of the primary use cases for the
    // user-provided closure, we must be prepared to handle faulty downstream
    // low-level hook implementations. As a fix, `send_key()` buffers input
    // events and sends them using `SendInput()` after returning from the closure.

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
        CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
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
                // Sends a Unicode character, known as `VK_PACKET`.
                // Interestingly, this is faster than sending a regular virtual key event.
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
        if layout.is_null() {
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
            // We have a virtual key but it is a dead-key, e.g.: `^` or `~` on international layouts.
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
