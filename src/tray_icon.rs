use std::{mem, ptr};

use widestring::u16cstr;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::winapi_util::MessageOnlyWindow;

pub struct TrayIcon {
    window: MessageOnlyWindow,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(
                NIM_DELETE,
                &mut Self::notification_data(self.window.handle()),
            );
        }
    }
}

impl TrayIcon {
    pub fn new(message: u32) -> Self {
        assert!(
            (WM_APP..WM_APP + 0x4000).contains(&message),
            "message must be in the WM_APP range"
        );

        let class_name = u16cstr!("trayicon").as_ptr();
        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());

            // A class is unique and the `RegisterClass()` function fails when
            // we create more than one tray icon but we do not care.
            let mut wnd_class: WNDCLASSW = mem::zeroed();
            wnd_class.lpfnWndProc = Some(Self::wndproc);
            wnd_class.hInstance = hinstance;
            wnd_class.lpszClassName = class_name;
            RegisterClassW(&wnd_class);
        }

        let window = MessageOnlyWindow::new(class_name);
        unsafe {
            // Set message as associated data
            SetWindowLongPtrW(window.handle(), GWLP_USERDATA, message as isize);

            // Create the tray icon
            let mut notification_data = Self::notification_data(window.handle());
            notification_data.uFlags = NIF_MESSAGE;
            notification_data.uCallbackMessage = WM_USER;
            Shell_NotifyIconW(NIM_ADD, &mut notification_data);

            Self { window }
        }
    }

    pub fn set_icon(&self, icon: HICON) {
        let mut notification_data = Self::notification_data(self.window.handle());
        notification_data.uFlags = NIF_ICON;
        notification_data.hIcon = icon;
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut notification_data);
        }
    }

    fn notification_data(hwnd: HWND) -> NOTIFYICONDATAW {
        unsafe {
            let mut notification_data: NOTIFYICONDATAW = mem::zeroed();
            notification_data.cbSize = mem::size_of_val(&notification_data) as _;
            notification_data.hWnd = hwnd;
            notification_data.uID = Self::message(hwnd);
            notification_data
        }
    }

    fn message(hwnd: HWND) -> u32 {
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _ }
    }

    // Forward the message to the main event loop.
    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_USER {
            PostMessageW(0, Self::message(hwnd), wparam, lparam);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
