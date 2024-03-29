//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::Cell;
use std::fmt::Display;
use std::{mem, ptr};

use encode_unicode::CharExt;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

type HookFn = dyn FnMut(KeyEvent) -> bool;

thread_local! {
    /// Stores the hook callback for the current thread.
    static HOOK: Cell<Option<Box<HookFn>>> = Cell::default();
}

/// Wrapper for the low-level keyboard hook API.
/// Automatically unregisters the hook when dropped.
pub struct KeyboardHook {
    handle: HHOOK,
}

impl KeyboardHook {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// The closure receives key press and key release events. When the closure
    /// returns `false` the key event is not modified and forwarded as if
    /// nothing happened. To ignore a key event or to remap it to another
    /// key return `true` and use [`send_key()`].
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediately be unregistered and not work."]
    pub fn set(callback: impl FnMut(KeyEvent) -> bool + 'static) -> KeyboardHook {
        assert!(
            HOOK.take().is_none(),
            "Only one keyboard hook can be registered per thread."
        );

        HOOK.set(Some(Box::new(callback)));

        let handle = unsafe { SetWindowsHookExA(WH_KEYBOARD_LL, Some(hook_proc), 0, 0) };
        assert_ne!(handle, 0, "Failed to install low-level keyboard hook.");
        KeyboardHook { handle }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            assert!(UnhookWindowsHookEx(self.handle) == TRUE);
            self.handle = 0;
        };
        HOOK.take();
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
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION as i32 {
        return CallNextHookEx(0, code, wparam, lparam);
    }

    let hook_lparam = &*(lparam as *const KBDLLHOOKSTRUCT);
    let key_event = KeyEvent::from_hook_lparam(hook_lparam);
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;

    // `SendInput()` internally calls the hook function. Filter out injected events
    // to prevent recursion and potential stack overflows if our remapping logic
    // sent the injected event.
    if injected {
        return CallNextHookEx(0, code, wparam, lparam);
    }

    let mut handled = false;

    // To call the closure registered for the keyboard hook we need to
    // take it out of the cell to get mutable access. When done we move
    // the closure back into the cell.
    // As long as we prevent recursion by dropping injected events, windows
    // should not be calling the hook again while it is already executing.
    // Which means we will never `take()` the cell twice.
    if let Some(mut h) = HOOK.take() {
        handled = h(key_event);
        HOOK.set(Some(h));
    } else {
        // There is one special case with classical CMD windows:
        // The "Quick Edit Mode" option which is enabled by default.
        // Windows stops to read from stdout and stderr when the user
        // selects characters in the cmd window.
        // Any write to stdout (e.g. a call to `println!()`) blocks while
        // "Quick Edit Mode" is active. The nasty part is that the key event
        // which exits the "Quick Edit Mode" triggers the hook a second time
        // before the blocked write to stdout can return.
    }

    if handled {
        -1
    } else {
        CallNextHookEx(0, code, wparam, lparam)
    }
}

/// Sends a virtual key event.
pub fn send_key(key: KeyEvent) {
    unsafe {
        let mut inputs: [INPUT; 2] = mem::zeroed();

        let n_inputs = match key.key {
            KeyType::VirtualKey(vk) => {
                inputs[0].r#type = INPUT_KEYBOARD;
                inputs[0].Anonymous.ki = key_input_from_event(key, vk.into());
                1
            }
            KeyType::Unicode(c) => {
                // Sends a unicode character, knows as `VK_PACKET`.
                // Interestingly this is faster than sending a regular virtual key event.
                inputs
                    .iter_mut()
                    .zip(c.to_utf16())
                    .map(|(input, c)| {
                        let mut kb_input: KEYBDINPUT = key_input_from_event(key, 0);
                        kb_input.wScan = c;
                        kb_input.dwFlags |= KEYEVENTF_UNICODE;
                        input.r#type = INPUT_KEYBOARD;
                        input.Anonymous.ki = kb_input;
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

fn key_input_from_event(key: KeyEvent, virtual_key: u16) -> KEYBDINPUT {
    KEYBDINPUT {
        wVk: virtual_key,
        wScan: key.scan_code,
        dwFlags: if key.up { KEYEVENTF_KEYUP } else { 0 },
        time: key.time,
        dwExtraInfo: 0,
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
