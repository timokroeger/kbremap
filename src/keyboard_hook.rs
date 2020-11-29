use std::{
    cell::RefCell, ffi::OsString, marker::PhantomData, mem, os::windows::ffi::OsStrExt, ptr,
};

use winapi::{
    ctypes::*,
    shared::{minwindef::*, windef::*},
    um::winuser::*,
};

thread_local!(static HOOK: RefCell<Option<Box<dyn FnMut(&KeyboardEvent) -> Remap>>> = RefCell::new(None));

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
    /// Returns `None` when called more than once from the same thread.
    pub fn set(callback: impl FnMut(&KeyboardEvent) -> Remap + 'a) -> Option<KeyboardHook<'a>> {
        HOOK.with(|hook| {
            if hook.borrow().is_some() {
                return None;
            }

            let boxed_cb: Box<dyn FnMut(&KeyboardEvent) -> Remap + 'a> = Box::new(callback);
            *hook.borrow_mut() = Some(unsafe { mem::transmute(boxed_cb) });

            Some(KeyboardHook {
                handle: unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), ptr::null_mut(), 0)
                        .as_mut()
                        .expect("Failed to install low-level keyboard hook.")
                },
                lifetime: PhantomData,
            })
        })
    }
}

impl<'a> Drop for KeyboardHook<'a> {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
        HOOK.with(|hook| hook.borrow_mut().take());
    }
}

#[repr(C)]
pub struct KeyboardEvent(KBDLLHOOKSTRUCT);

impl KeyboardEvent {
    /// Key was released.
    pub fn up(&self) -> bool {
        self.0.flags & LLKHF_UP != 0
    }

    /// Key was pressed.
    pub fn down(&self) -> bool {
        !self.up()
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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Remap {
    Transparent,
    Ignore,
    Character(char),
    VirtualKey(u8),
}

unsafe extern "system" fn hook_proc(code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code != 0 {
        return -1;
    }

    let kb_event = &*(l_param as *const KeyboardEvent);

    // If the user calls `SendInput()` in the callback the hook is retriggered immediatelly with
    // an injected key event. Even though the execution context still is in the same thread we
    // need to filter out injected events to prevent recursion.
    if kb_event.is_injected() {
        println!(
            ", injected as (sc: {:#06X}, vk: {:#04X})",
            kb_event.scan_code(),
            kb_event.virtual_key()
        );
        return CallNextHookEx(ptr::null_mut(), code, w_param, l_param);
    }

    print!(
        "{} (sc: {:#06X}, vk: {:#04X}) ",
        if kb_event.up() { '↑' } else { '↓' },
        kb_event.scan_code(),
        kb_event.virtual_key(),
    );

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
            send_char(kb_event, c);
        }
        Remap::VirtualKey(vk) => {
            print!("remapped to virtual key `{:#04X}", vk);
            send_key(kb_event, vk);
        }
    }

    -1
}

fn send_unicode(kb_event: &KeyboardEvent, c: u16) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wScan = c;
        kb_input.dwFlags = KEYEVENTF_UNICODE;
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

fn send_char(kb_event: &KeyboardEvent, c: char) {
    unsafe {
        // Convert to UTF-16
        let c16 = OsString::from(c.to_string()).encode_wide().next().unwrap();

        let vk_state = VkKeyScanW(c16);
        let dead_key = MapVirtualKeyW((vk_state & 0xFF) as u32, MAPVK_VK_TO_CHAR) & 0x80000000 != 0;

        // Check if the modifer keys, which are required to type the character, are pressed.
        let mut modifiers_matching = true;
        let modifier_pressed = |vk| (GetAsyncKeyState(vk) as u16) & 0x8000 != 0;

        let shift = vk_state & 0x100 != 0;
        if shift {
            modifiers_matching = modifier_pressed(VK_SHIFT);
        }

        let ctrl = vk_state & 0x200 != 0;
        if ctrl {
            modifiers_matching = modifier_pressed(VK_CONTROL);
        }

        let alt = vk_state & 0x400 != 0;
        if alt {
            modifiers_matching = modifier_pressed(VK_MENU);
        }

        // Send the character as unicode input if:
        // 1. There is no key for the character available on the current keyboard layout
        // 2. Cannot be pressed with the currently active modifiers
        // 3. The key for this character is a dead key (diacritic)
        if vk_state == -1 || !modifiers_matching || dead_key {
            print!("remapped to `{}` as unicode input", c);
            send_unicode(kb_event, c16);
        } else {
            print!("remapped to `{}` as virtual key", c);
            send_key(kb_event, vk_state as u8);
        }
    }
}
