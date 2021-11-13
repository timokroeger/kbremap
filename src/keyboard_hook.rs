//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::RefCell;
use std::{mem, ptr};

use encode_unicode::error::InvalidUtf16Slice;
use encode_unicode::{CharExt, Utf16Char};
use winapi::ctypes::*;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::winuser::*;

thread_local! {
    /// Stores the hook callback for the current thread.
    static HOOK: RefCell<Option<Box<dyn FnMut(&KeyboardEvent) -> Remap>>> = RefCell::new(None)
}

/// Wrapper for the low-level keyboard hook API.
/// Automatically unregisters the hook when dropped.
pub struct KeyboardHook {
    handle: HHOOK,
}

impl KeyboardHook {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// Remaps the key event to a unicode character if the closure returns `Some`.
    /// Sends the character with a single virtual key if the character is available
    /// on the current layout. Uses `VK_PACKET` to pass Unicode characters as if
    /// they were keystrokes for everything else.
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediatelly be unregistered and not work."]
    pub fn set(callback: impl FnMut(&KeyboardEvent) -> Remap + 'static) -> KeyboardHook {
        HOOK.with(|hook| {
            assert!(
                hook.borrow().is_none(),
                "Only one keyboard hook can be registered per thread."
            );

            *hook.borrow_mut() = Some(Box::new(callback));

            KeyboardHook {
                handle: unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), ptr::null_mut(), 0)
                        .as_mut()
                        .expect("Failed to install low-level keyboard hook.")
                },
            }
        })
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
        HOOK.with(|hook| hook.take());
    }
}

/// Safe wrapper to access information about a keyboard event.
/// Passed as argument to the user provieded hook callback.
#[repr(C)]
pub struct KeyboardEvent(KBDLLHOOKSTRUCT);

impl KeyboardEvent {
    /// Key was released.
    pub fn up(&self) -> bool {
        self.0.flags & LLKHF_UP != 0
    }

    /// Scan code as defined by the keyboard.
    /// Extended keycodes have the three most significant bits set (0xExxx).
    pub fn scan_code(&self) -> u16 {
        (if self.0.flags & LLKHF_EXTENDED != 0 {
            self.0.scanCode | 0xE000
        } else {
            self.0.scanCode
        }) as _
    }

    /// Virtual key as defined by the layout set by windows.
    ///
    /// <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    pub fn virtual_key(&self) -> u8 {
        self.0.vkCode as _
    }

    /// Time in milliseconds since boot.
    pub fn time(&self) -> u32 {
        self.0.time
    }

    fn is_injected(&self) -> bool {
        self.0.flags & LLKHF_INJECTED != 0
    }
}

/// Remap action associated with the key. Returned by the user provided hook callback.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Remap {
    /// Do not remap the key. Forwards the key event without any changes.
    Transparent,

    /// Ignores the key event.
    Ignore,

    /// Sends a (Unicode) character, if possible as key press,
    /// Ignores the original key event.
    Character(char),

    /// Sends a virtual key press.
    /// Ignores the original key event.
    /// Reference: <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    VirtualKey(u8),
}

/// The actual WinAPI compatible callback.
unsafe extern "system" fn hook_proc(code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code != 0 {
        return -1;
    }

    let kb_event = &*(l_param as *const KeyboardEvent);

    // `SendInput()` internally calls the hook function. Filter out injected events
    // to prevent recursion and potential stack overflows if our remapping logic
    // sent the injected event.
    if kb_event.is_injected() {
        match Utf16Char::from_slice_start(&[kb_event.scan_code()]) {
            Err(InvalidUtf16Slice::MissingSecond) => print!(
                ", injected as (sc: {:#06X}, vk: {:#04X}) and ",
                kb_event.scan_code(),
                kb_event.virtual_key()
            ),
            Err(InvalidUtf16Slice::FirstLowSurrogate) => println!(
                "(sc: {:#06X}, vk: {:#04X})",
                kb_event.scan_code(),
                kb_event.virtual_key()
            ),
            _ => println!(
                ", injected as (sc: {:#06X}, vk: {:#04X})",
                kb_event.scan_code(),
                kb_event.virtual_key()
            ),
        }

        return CallNextHookEx(ptr::null_mut(), code, w_param, l_param);
    }

    print!(
        "{} (sc: {:#06X}, vk: {:#04X}) ",
        if kb_event.up() { '↑' } else { '↓' },
        kb_event.scan_code(),
        kb_event.virtual_key(),
    );

    // The unwrap cannot fail, because windows only calls this function after
    // registering the hook (before which we have set [`HOOK`]).
    // The mutable reference can be taken as long as we properly prevent recursion
    // by dropping injected events.
    let remap = HOOK.with(|hook| hook.borrow_mut().as_mut().unwrap()(kb_event));

    match remap {
        Remap::Transparent => {
            println!("forwarded");
            return CallNextHookEx(ptr::null_mut(), code, w_param, l_param);
        }
        Remap::Ignore => {
            println!("ignored");
        }
        Remap::Character(c) => {
            if let Some(vk) = get_virtual_key(c) {
                print!("remapped to `{}` as virtual key", c);
                send_key(kb_event, vk);
            } else {
                print!("remapped to `{}` as unicode input", c);
                send_unicode(kb_event, c);
            }
        }
        Remap::VirtualKey(vk) => {
            print!("remapped to virtual key {:#04X}", vk);
            send_key(kb_event, vk);
        }
    }

    -1
}

/// Injects a unicode character, knows as `VK_PACKET`.
/// Interestingly this is faster than sending a regular virtual key event.
fn send_unicode(kb_event: &KeyboardEvent, c: char) {
    unsafe {
        let mut inputs: [INPUT; 2] = mem::zeroed();
        let n_inputs = inputs
            .iter_mut()
            .zip(c.to_utf16())
            .map(|(input, c)| {
                let mut kb_input: KEYBDINPUT = mem::zeroed();
                kb_input.wScan = c;
                kb_input.dwFlags = KEYEVENTF_UNICODE;
                if kb_event.up() {
                    kb_input.dwFlags |= KEYEVENTF_KEYUP;
                }
                kb_input.time = kb_event.time();
                input.type_ = INPUT_KEYBOARD;
                *input.u.ki_mut() = kb_input;
            })
            .count();

        SendInput(
            n_inputs as u32,
            &mut inputs[0],
            mem::size_of::<INPUT>() as _,
        );
    }
}

/// Injects a virtual key press.
fn send_key(kb_event: &KeyboardEvent, vk: u8) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wVk = vk as u16;
        kb_input.wScan = kb_event.scan_code();
        if kb_event.up() {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }
        kb_input.time = kb_event.time();

        let mut input: INPUT = mem::zeroed();
        input.type_ = INPUT_KEYBOARD;
        *input.u.ki_mut() = kb_input;

        SendInput(1, &mut input, mem::size_of_val(&input) as _);
    }
}

/// Returns a virtual key code if the requested character can be typed with a
/// single key press/release.
fn get_virtual_key(c: char) -> Option<u8> {
    unsafe {
        let layout = GetKeyboardLayout(GetWindowThreadProcessId(
            GetForegroundWindow(),
            ptr::null_mut(),
        ));
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

        // Check if the modifer keys, which are required to type the character, are pressed.
        let modifier_pressed = |vk| (GetKeyState(vk) as u16) & 0x8000 != 0;

        let shift = vk_state & 0x100 != 0;
        let caps_lock_enabled = (GetKeyState(VK_CAPITAL) as u16) & 0x0001 != 0;
        if shift && (modifier_pressed(VK_SHIFT) == caps_lock_enabled) {
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
