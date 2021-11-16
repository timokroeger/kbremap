//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::{mem, ptr};

use encode_unicode::CharExt;
use winapi::ctypes::*;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::winuser::*;

type HookFn<'a> = dyn FnMut(KeyEvent) -> Remap + 'a;

thread_local! {
    /// Stores the hook callback for the current thread.
    static HOOK_STATE: RefCell<HookState> = RefCell::default();
}

#[derive(Default)]
struct HookState {
    hook: Option<Box<HookFn<'static>>>,
}

/// Wrapper for the low-level keyboard hook API.
/// Automatically unregisters the hook when dropped.
pub struct KeyboardHook<'a> {
    handle: HHOOK,
    lifetime: PhantomData<&'a ()>,
}

impl<'a> KeyboardHook<'a> {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// Remaps the key event to a unicode character if the closure returns `Some`.
    /// Sends the character with a single virtual key if the character is available
    /// on the current layout. Uses `VK_PACKET` to pass Unicode characters as if
    /// they were keystrokes for everything else.
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediatelly be unregistered and not work."]
    pub fn set(callback: impl FnMut(KeyEvent) -> Remap + 'a) -> KeyboardHook<'a> {
        HOOK_STATE.with(|state| {
            let mut state = state.borrow_mut();
            assert!(
                state.hook.is_none(),
                "Only one keyboard hook can be registered per thread."
            );

            // The rust compiler needs type annotations to create a trait object rather than a
            // specialized boxed closure so that we can use transmute in the next step.
            let boxed_cb: Box<HookFn<'a>> = Box::new(callback);

            // Safety: Transmuting to 'static lifetime is required to put the closure in thread
            // local storage. It is safe to do so because we properly unregister the hook on drop
            // after which the global (thread local) variable `HOOK` will not be acccesed anymore.
            state.hook = Some(unsafe { mem::transmute(boxed_cb) });

            KeyboardHook {
                handle: unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), ptr::null_mut(), 0)
                        .as_mut()
                        .expect("Failed to install low-level keyboard hook.")
                },
                lifetime: PhantomData,
            }
        })
    }
}

impl<'a> Drop for KeyboardHook<'a> {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
        HOOK_STATE.with(|state| state.take());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// Virtual key as defined by the layout set by windows.
    ///
    /// <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    pub virtual_key: u8,

    /// Scan code as defined by the keyboard.
    /// Extended keycodes have the three most significant bits set (0xExxx).
    pub scan_code: u16,

    /// Key was released
    pub up: bool,

    /// Time in milliseconds since boot.
    pub time: u32,
}

impl KeyEvent {
    pub fn from_hook_lparam(lparam: &KBDLLHOOKSTRUCT) -> Self {
        let mut scan_code = lparam.scanCode as u16;
        if lparam.flags & LLKHF_EXTENDED != 0 {
            scan_code |= 0xE000;
        }

        Self {
            virtual_key: lparam.vkCode as _,
            scan_code,
            up: lparam.flags & LLKHF_UP != 0,
            time: lparam.time,
        }
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
unsafe extern "system" fn hook_proc(code: c_int, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code != HC_ACTION {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let hook_lparam = &*(lparam as *const KBDLLHOOKSTRUCT);
    let injected = hook_lparam.flags & LLKHF_INJECTED != 0;
    let key = KeyEvent::from_hook_lparam(hook_lparam);

    print!(
        "{} (sc: {:#06X}, vk: {:#04X}) ",
        if key.up { '↑' } else { '↓' },
        key.scan_code,
        key.virtual_key
    );

    // `SendInput()` internally calls the hook function. Filter out injected events
    // to prevent recursion and potential stack overflows if our remapping logic
    // sent the injected event.
    if injected {
        println!("injected");
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let remap = HOOK_STATE.with(|state| {
        // The mutable reference can be taken as long as we properly prevent recursion
        // by dropping injected events.
        let mut state = state.borrow_mut();

        // The unwrap cannot fail, because windows only calls this function after
        // registering the hook (before which we have set [`HOOK_STATE`]).
        state.hook.as_mut().unwrap()(key)
    });

    match remap {
        Remap::Transparent => {
            println!("forwarded");
            return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
        }
        Remap::Ignore => {
            println!("ignored");
        }
        Remap::Character(c) => {
            if let Some(virtual_key) = get_virtual_key(c) {
                println!("remapped to `{}` as virtual key", c);
                send_key(KeyEvent { virtual_key, ..key });
            } else {
                println!("remapped to `{}` as unicode input", c);
                send_unicode(key, c);
            }
        }
        Remap::VirtualKey(virtual_key) => {
            println!("remapped to virtual key {:#04X}", virtual_key);
            send_key(KeyEvent { virtual_key, ..key });
        }
    }

    -1
}

/// Injects a unicode character, knows as `VK_PACKET`.
/// Interestingly this is faster than sending a regular virtual key event.
fn send_unicode(key: KeyEvent, c: char) {
    unsafe {
        let mut inputs: [INPUT; 2] = mem::zeroed();
        let n_inputs = inputs
            .iter_mut()
            .zip(c.to_utf16())
            .map(|(input, c)| {
                let mut kb_input: KEYBDINPUT = mem::zeroed();
                kb_input.wScan = c;
                kb_input.dwFlags = KEYEVENTF_UNICODE;
                if key.up {
                    kb_input.dwFlags |= KEYEVENTF_KEYUP;
                }
                kb_input.time = key.time;
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
fn send_key(key: KeyEvent) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wVk = key.virtual_key.into();
        kb_input.wScan = key.scan_code;
        if key.up {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }
        kb_input.time = key.time;

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
