//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::Cell;
use std::fmt::Display;
use std::marker::PhantomData;
use std::{mem, ptr};

use encode_unicode::CharExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Threading::{TrySubmitThreadpoolCallback, PTP_CALLBACK_INSTANCE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

thread_local! {
    /// Stores a type-erased pointer to the hook closure.
    static HOOK: Cell<*mut ()> = const { Cell::new(ptr::null_mut()) };
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
            HOOK.get().is_null(),
            "Only one keyboard hook can be registered per thread."
        );

        let callback = Box::into_raw(Box::new(callback));
        HOOK.set(callback as *mut ());

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
            drop(Box::from_raw(HOOK.replace(ptr::null_mut()) as *mut F));
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
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;

    // `SendInput()` internally triggers the hook function. Filter out injected
    // events to prevent an infinite loop if our remapping logic has sent the
    // injected event.
    if injected {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    // Windows re-enters the hook function for two conditions:
    // 1. `SendInput()` called from within the hook, which produces an injected
    //    message. We pass on injected messages without looking at them anyway.
    // 2. The hook blocks longer than the number of ms specified in the registry
    //    key `HKEY_CURRENT_USER\Control Panel\LowLevelHooksTimeout`.

    // As the main use case for a low-level keyboard hook is to remap keys,
    // we implement a non-blocking `send_key()` function (see below) which can
    // safely be called from the hook.

    // The TLS `HOOK` variable contains a null pointer while the user-provided
    // closure is running. We can use that to detect if the hook was re-entered.
    // Only call the closure when we are sure it is available.
    let hook_ptr = HOOK.replace(ptr::null_mut()) as *mut F;
    if let Some(hook) = unsafe { hook_ptr.as_mut() } {
        let handled = hook(KeyEvent::from_hook_lparam(hook_lparam));
        HOOK.set(hook_ptr as *mut ());
        if handled {
            return -1;
        }
    }

    CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
}

/// Sends a virtual key event.
pub fn send_key(key: KeyEvent) {
    // `SendInput()` may block if a slow/faulty low-level keyboard hook is
    // registered. As a fix, forwards the key event to another thread to call
    // `SendInput()` from there. That way our hook is unaffected by other
    // faulty hooks and Windows can correctly remove the offending hooks from
    // the hook chain.
    unsafe {
        TrySubmitThreadpoolCallback(
            Some(send_key_callback),
            Box::into_raw(Box::new(key)) as *mut _,
            ptr::null(),
        );
    }
}

unsafe extern "system" fn send_key_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut core::ffi::c_void,
) {
    unsafe {
        let key = Box::from_raw(context as *mut KeyEvent);
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
