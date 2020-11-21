use std::{cell::UnsafeCell, ptr};

use winapi::{
    ctypes::*,
    shared::{minwindef::*, windef::*},
    um::winuser::*,
};

/// Receives the scan codes of a keyboard event from the low level keyboard hook.
///
/// If the callback returns `false`, the key event will not be forwarded to other appliations.
pub type Callback = fn(scan_code: u32, up: bool) -> bool;

thread_local! {
    static KEYBOARD_HOOK: UnsafeCell<KeyboardHook> = UnsafeCell::new(KeyboardHook {
        handle: ptr::null_mut(),
        callback: |_, _| true,
    })
}

pub struct KeyboardHook {
    handle: HHOOK,
    callback: Callback,
}

impl KeyboardHook {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// Calls `callback` when receiving keyboard input events.
    /// Filters out injected and extended scan codes.
    ///
    /// # Panics
    ///
    /// Panics when called more than once from the same thread.
    pub fn set(callback: Callback) {
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
            kbh.callback = callback;
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
        if injected || extended || ((*kbh.get()).callback)(input_event.scanCode, up) {
            CallNextHookEx(ptr::null_mut(), code, w_param, l_param)
        } else {
            -1
        }
    })
}
