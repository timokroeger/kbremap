use std::{mem, ptr};

use wchar::wchz;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::*;
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

pub fn icon_from_rc_numeric(id: u16) -> HICON {
    let hicon =
        unsafe { LoadImageW(GetModuleHandleW(ptr::null()), id as _, IMAGE_ICON, 0, 0, 0) as _ };
    assert_ne!(hicon, ptr::null_mut(), "icon resource {} not found", id);
    hicon
}

pub fn popupmenu_from_rc_numeric(id: u16) -> HMENU {
    unsafe {
        let menu = LoadMenuA(GetModuleHandleA(ptr::null()), id as _);
        assert_ne!(menu, ptr::null_mut(), "menu resource {} not found", id);
        let submenu = GetSubMenu(menu, 0);
        assert_ne!(
            submenu,
            ptr::null_mut(),
            "menu resource {} requires a popup submenu item",
            id
        );
        submenu
    }
}

pub fn create_dummy_window() -> MessageOnlyWindow {
    unsafe {
        let mut wnd_class: WNDCLASSW = mem::zeroed();
        wnd_class.lpfnWndProc = Some(DefWindowProcW);
        wnd_class.lpszClassName = wchz!("menu").as_ptr();
        let wnd_class_atom = RegisterClassW(&wnd_class);
        assert_ne!(wnd_class_atom, 0);

        MessageOnlyWindow::new(wnd_class_atom as _)
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
