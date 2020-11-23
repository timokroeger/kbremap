use std::{cell::RefCell, marker::PhantomData, mem, ptr};

use winapi::{
    ctypes::*,
    shared::{minwindef::*, windef::*},
    um::winuser::*,
};

thread_local!(static HOOK: RefCell<Option<Box<dyn FnMut(&KeyboardEvent) -> bool>>> = RefCell::new(None));

pub struct KeyboardHook<'a> {
    handle: HHOOK,
    lifetime: PhantomData<&'a ()>,
}

impl<'a> KeyboardHook<'a> {
    /// Sets the low-level keyboard hook for this thread.
    ///
    /// If the closure returns `false`, the key event will not be forwarded to other
    /// appliations.
    ///
    /// Returns `None` when called more than once from the same thread.
    pub fn set(callback: impl FnMut(&KeyboardEvent) -> bool + 'a) -> Option<KeyboardHook<'a>> {
        HOOK.with(|hook| {
            if hook.borrow().is_some() {
                return None;
            }

            let boxed_cb: Box<dyn FnMut(&KeyboardEvent) -> bool + 'a> = Box::new(callback);
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
    pub fn scan_code(&self) -> u16 {
        self.0.scanCode as _
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

    pub fn is_extended(&self) -> bool {
        self.0.flags & LLKHF_EXTENDED != 0
    }
}

unsafe extern "system" fn hook_proc(code: c_int, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code != 0 {
        return -1;
    }

    // If the user calls `SendInput()` in the callback the hook is retriggered immediatelly with
    // an injected key event. Even though the execution context still is in the same thread we
    // need to filter out injected events to prevent recursion.
    let kb_event = &*(l_param as *const KeyboardEvent);
    if kb_event.is_injected() {
        return CallNextHookEx(ptr::null_mut(), code, w_param, l_param);
    }

    HOOK.with(|hook| {
        let call_next_hook = hook.borrow_mut().as_mut().unwrap()(kb_event);
        if call_next_hook {
            CallNextHookEx(ptr::null_mut(), code, w_param, l_param)
        } else {
            // Swallow the key event
            -1
        }
    })
}
