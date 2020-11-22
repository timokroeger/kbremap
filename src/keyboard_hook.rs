use std::{cell::UnsafeCell, ptr};

use winapi::{
    ctypes::*,
    shared::{minwindef::*, windef::*},
    um::winuser::*,
};

thread_local! {
    static KEYBOARD_HOOK: UnsafeCell<KeyboardHook> = UnsafeCell::new(KeyboardHook {
        handle: ptr::null_mut(),
        callback: Box::new(|_, _| true),
    })
}

pub struct KeyboardHook {
    handle: HHOOK,
    callback: Box<dyn FnMut(u16, bool) -> bool>,
}

impl KeyboardHook {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// Filters out injected and extended scan codes before passing received
    /// keyboard scan codes the the provided closure. The first closure parameter
    /// is the scan code as defined by the OS. The second parameter is `false` for
    /// key down (press) events and `true` for key up (release) events.
    /// If the closure returns `false`, the key event will not be forwarded to other
    /// appliations.
    ///
    /// # Panics
    ///
    /// Panics when called more than once from the same thread.
    pub fn set(callback: impl FnMut(u16, bool) -> bool + 'static) {
        KEYBOARD_HOOK.with(|kbh| {
            let kbh = unsafe { &mut *kbh.get() };
            assert!(
                kbh.handle.is_null(),
                "Only one keyboard hook can be set per thread"
            );

            kbh.handle = unsafe {
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), ptr::null_mut(), 0)
                    .as_mut()
                    .expect("Failed to install low-level keyboard hook.")
            };
            kbh.callback = Box::new(callback);
        })
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe { UnhookWindowsHookEx(self.handle) };
    }
}

pub struct KeyboardEvent<'a>(&'a KBDLLHOOKSTRUCT);

impl<'a> KeyboardEvent<'a> {
    /// Key was released.
    pub fn up(&self) -> bool {
        self.0.flags & LLKHF_UP != 0
    }

    /// Key was pressed.
    pub fn down(&self) -> bool {
        !self.up()
    }

    /// Scan code as defined by the keyboard.
    pub fn scan_code(&self) -> u16 {
        self.0.scanCode as _
    }

    /// Virtual key as defined by the layout set by windows.
    ///
    /// https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes
    pub fn virtual_key(&self) -> u8 {
        self.0.vkCode as _
    }

    /// Time in milliseconds since boot.
    pub fn time(&self) -> u32 {
        self.0.time
    }

    pub fn is_injected(&self) -> bool {
        self.0.flags & LLKHF_INJECTED != 0
    }

    pub fn is_extended(&self) -> bool {
        self.0.flags & LLKHF_EXTENDED != 0
    }
}

unsafe extern "system" fn hook_proc(code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code != 0 {
        return -1;
    }

    let input_event = KeyboardEvent(&*(l_param as *const _));

    println!(
        "{}{}{} scan code: {:#06X}, virtual key: {:#04X}",
        if input_event.up() { '↑' } else { '↓' },
        if input_event.is_injected() { 'i' } else { ' ' },
        if input_event.is_extended() { 'e' } else { ' ' },
        input_event.scan_code(),
        input_event.virtual_key(),
    );

    KEYBOARD_HOOK.with(|kbh| {
        if input_event.is_injected()
            || input_event.is_extended()
            || ((*kbh.get()).callback)(input_event.scan_code(), input_event.up()) {
            CallNextHookEx(ptr::null_mut(), code, w_param, l_param)
        } else {
            -1
        }
    })
}
