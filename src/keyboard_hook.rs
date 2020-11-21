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

unsafe extern "system" fn hook_proc(code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code != 0 {
        return -1;
    }

    let input_event = (l_param as *const KBDLLHOOKSTRUCT).as_ref().unwrap();
    let up = input_event.flags & LLKHF_UP != 0;
    let injected = input_event.flags & LLKHF_INJECTED != 0;
    let extended = input_event.flags & LLKHF_EXTENDED != 0;

    println!(
        "{}{}{} scan code: {:#04X}, virtual key: {:#04X}",
        if up { '↑' } else { '↓' },
        if injected { 'i' } else { ' ' },
        if extended { 'e' } else { ' ' },
        input_event.scanCode,
        input_event.vkCode
    );

    KEYBOARD_HOOK.with(|kbh| {
        if injected || extended || ((*kbh.get()).callback)(input_event.scanCode as _, up) {
            CallNextHookEx(ptr::null_mut(), code, w_param, l_param)
        } else {
            -1
        }
    })
}
