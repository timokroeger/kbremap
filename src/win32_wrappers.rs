use std::env;
use std::ffi::OsString;
use std::os::windows::prelude::{OsStrExt, OsStringExt};
use std::{mem, ptr};

use wchar::wchz;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::*;
use winapi::um::winnt::*;
use winapi::um::winreg::*;
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
        unsafe { LoadImageW(GetModuleHandleW(ptr::null()), id as _, IMAGE_ICON, 0, 0, 0).cast() };
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

pub struct AutoStartEntry<'a> {
    key: HKEY,
    name: &'a [u16],
}

impl<'a> Drop for AutoStartEntry<'a> {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.key);
        }
    }
}

impl<'a> AutoStartEntry<'a> {
    pub fn new(name: &'a [u16]) -> Self {
        unsafe {
            let mut key: HKEY = mem::zeroed();
            RegCreateKeyW(
                HKEY_CURRENT_USER,
                wchz!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run") as _,
                &mut key,
            );
            Self { key, name }
        }
    }

    pub fn is_registered(&self) -> bool {
        let exe_path = env::current_exe().unwrap();
        unsafe {
            let mut path_buf = [0_u16; MAX_PATH];
            let mut path_len = mem::size_of_val(&path_buf) as u32;
            let key_exists = RegGetValueW(
                self.key,
                ptr::null(),
                self.name.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                &mut path_buf as *mut _ as _,
                &mut path_len as _,
            ) == 0;

            key_exists
                && OsString::from_wide(&path_buf[..(path_len as usize / 2 - 1)])
                    == exe_path.as_os_str()
        }
    }

    pub fn register(&self) {
        let mut exe_path: Vec<u16> = env::current_exe()
            .unwrap()
            .as_os_str()
            .encode_wide()
            .collect();
        exe_path.push(0);

        unsafe {
            RegSetValueExW(
                self.key,
                self.name.as_ptr(),
                0,
                REG_SZ,
                exe_path.as_mut_ptr() as _,
                (exe_path.len() * 2) as _,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueW(self.key, self.name.as_ptr());
        }
    }
}
