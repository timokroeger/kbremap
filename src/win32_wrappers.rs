use std::{mem, ptr};

use winapi::shared::windef::*;
use winapi::um::winnt::*;
use winapi::um::winuser::*;

pub struct MessageOnlyWindow(HWND);

impl Drop for MessageOnlyWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

impl MessageOnlyWindow {
    pub fn new(class_name: LPCWSTR) -> Self {
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                class_name,
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            assert_ne!(hwnd, ptr::null_mut());
            Self(hwnd)
        }
    }

    pub fn handle(&self) -> HWND {
        self.0
    }
}

pub fn message_loop(mut cb: impl FnMut(&MSG)) -> i32 {
    unsafe {
        let mut msg = mem::zeroed();
        loop {
            match GetMessageW(&mut msg, ptr::null_mut(), 0, 0) {
                1 => {
                    cb(&msg);
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                0 => return msg.wParam as _,
                _ => unreachable!(),
            }
        }
    }
}
