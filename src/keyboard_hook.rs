//! Safe abstraction over the low-level windows keyboard hook API.

use std::cell::RefCell;
use std::fmt::Display;
use std::marker::PhantomData;
use std::{mem, ptr};

use encode_unicode::CharExt;
use winapi::ctypes::*;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::winuser::*;

type HookFn<'a> = dyn FnMut(KeyEvent) -> Option<KeyAction> + 'a;

thread_local! {
    /// Stores the hook callback for the current thread.
    static HOOK_STATE: RefCell<HookState> = RefCell::default();
}

#[derive(Default)]
struct HookState {
    hook: Option<Box<HookFn<'static>>>,
    disable_caps_lock: bool,
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
    /// The closure receives key press and key release events. When the closure
    /// returns `None` they key event is not modified and forwarded to processes
    /// is if nothing happened. To ignore a key event or to remap it to another
    /// key return a [`KeyAction`].
    ///
    /// Character actions are sent with a single virtual key event if the character
    /// is available on the current system keyboard layout.
    /// Uses `VK_PACKET` to remap a key to Unicode codepoint if no dedicated key
    /// for that character exists.
    ///
    /// Panics when a hook is already registered from the same thread.
    #[must_use = "The hook will immediatelly be unregistered and not work."]
    pub fn set(callback: impl FnMut(KeyEvent) -> Option<KeyAction> + 'a) -> KeyboardHook<'a> {
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

    /// Caps lock is automatically disabled when set to `true`.
    ///
    /// Useful when the caps lock state is toggled externally for example by RDP
    /// or other programs running with admin rights.
    pub fn disable_caps_lock(&self, val: bool) {
        HOOK_STATE.with(|state| state.borrow_mut().disable_caps_lock = val);
    }
}

impl<'a> Drop for KeyboardHook<'a> {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
        HOOK_STATE.with(|state| state.take());
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

/// Action associated with the key. Returned by the user provided hook callback.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KeyAction {
    /// Do not forward or send a key action.
    Ignore,

    /// Sends a (Unicode) character, if possible as virtual key press.
    Character(char),

    /// Sends a virtual key press.
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
    let mut key_event = KeyEvent::from_hook_lparam(hook_lparam);

    // `SendInput()` internally calls the hook function. Filter out injected events
    // to prevent recursion and potential stack overflows if our remapping logic
    // sent the injected event.
    if injected {
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    let remap = HOOK_STATE.with(|state| {
        // The mutable reference can be taken as long as we properly prevent recursion
        // by dropping injected events.
        let mut state = state.borrow_mut();
        if state.disable_caps_lock && caps_lock_enabled() {
            println!("disabling caps lock");
            send_key(KeyEvent {
                up: false,
                key: KeyType::VirtualKey(VK_CAPITAL as _),
                ..key_event
            });
            send_key(KeyEvent {
                up: true,
                key: KeyType::VirtualKey(VK_CAPITAL as _),
                ..key_event
            });
        }

        // The unwrap cannot fail, because windows only calls this function after
        // registering the hook (before which we have set [`HOOK_STATE`]).
        state.hook.as_mut().unwrap()(key_event)
    });

    let mut log_line = key_event.to_string();

    if remap.is_none() {
        log_line.push_str("forwarded");
        return CallNextHookEx(ptr::null_mut(), code, wparam, lparam);
    }

    match remap.unwrap() {
        KeyAction::Ignore => {
            log_line.push_str("ignored");
            return -1;
        }
        KeyAction::Character(c) => {
            if let Some(virtual_key) = get_virtual_key(c) {
                log_line = format!("{} remapped to `{}` as virtual key", log_line, c);
                key_event.key = KeyType::VirtualKey(virtual_key);
            } else {
                log_line = format!("{} remapped to `{}` as unicode input", log_line, c);
                key_event.key = KeyType::Unicode(c);
            }
        }
        KeyAction::VirtualKey(virtual_key) => {
            log_line = format!("{} remapped to virtual key {:#04X}", log_line, virtual_key);
            key_event.key = KeyType::VirtualKey(virtual_key);
        }
    }

    tracing::debug!("{}", log_line);

    send_key(key_event);

    -1
}

fn send_key(key: KeyEvent) {
    unsafe {
        let mut inputs: [INPUT; 2] = mem::zeroed();

        let n_inputs = match key.key {
            KeyType::VirtualKey(vk) => {
                let mut kb_input = key_input_from_event(key);
                kb_input.wVk = vk.into();

                inputs[0].type_ = INPUT_KEYBOARD;
                *inputs[0].u.ki_mut() = kb_input;
                1
            }
            KeyType::Unicode(c) => {
                // Sends a unicode character, knows as `VK_PACKET`.
                // Interestingly this is faster than sending a regular virtual key event.
                inputs
                    .iter_mut()
                    .zip(c.to_utf16())
                    .map(|(input, c)| {
                        let mut kb_input: KEYBDINPUT = key_input_from_event(key);
                        kb_input.wScan = c;
                        kb_input.dwFlags |= KEYEVENTF_UNICODE;
                        input.type_ = INPUT_KEYBOARD;
                        *input.u.ki_mut() = kb_input;
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

fn key_input_from_event(key: KeyEvent) -> KEYBDINPUT {
    let mut kb_input: KEYBDINPUT = unsafe { mem::zeroed() };
    if key.up {
        kb_input.dwFlags |= KEYEVENTF_KEYUP;
    }
    kb_input.time = key.time;
    kb_input
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

fn caps_lock_enabled() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL) as u16) & 0x0001 != 0 }
}
