use std::{mem, ptr};

use wchar::wchz;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::*;
use winapi::um::shellapi::*;
use winapi::um::winuser::*;

use crate::win32_wrappers::MessageOnlyWindow;

const WM_USER_TRAYICON: UINT = WM_USER + 873;

#[derive(Debug)]
pub enum Event {
    DoubleClick,
    RightClick,
}

#[derive(Debug)]
pub struct EventMessage {
    pub event: Event,
    pub x: i16,
    pub y: i16,
}

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
            message >= WM_APP && message < WM_APP + 0x4000,
            "message must be in the WM_APP range"
        );

        unsafe {
            let hinstance = GetModuleHandleW(ptr::null());

            let class_name = wchz!("trayicon").as_ptr();

            // A class is unique and the `RegisterClass()` function fails when
            // we create more than one tray icon but we do not care.
            let mut wnd_class: WNDCLASSW = mem::zeroed();
            wnd_class.lpfnWndProc = Some(Self::wndproc);
            wnd_class.hInstance = hinstance;
            wnd_class.lpszClassName = class_name;
            RegisterClassW(&wnd_class);

            let window = MessageOnlyWindow::new(class_name);

            // Set message as associated data
            SetWindowLongPtrW(window.handle(), GWLP_USERDATA, message as _);

            // Create the tray icon
            let mut notification_data = Self::notification_data(window.handle());
            notification_data.uFlags = NIF_MESSAGE;
            notification_data.uCallbackMessage = WM_USER_TRAYICON;
            Shell_NotifyIconW(NIM_ADD, &mut notification_data);

            *notification_data.u.uVersion_mut() = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &mut notification_data);

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

    pub fn event_from_message(&self, msg: &MSG) -> Option<EventMessage> {
        if msg.message != Self::message(self.window.handle()) {
            return None;
        }

        let event = match msg.lParam as u32 & 0xFFFF {
            WM_LBUTTONDBLCLK => Event::DoubleClick,
            WM_RBUTTONUP => Event::RightClick,
            _ => return None,
        };

        Some(EventMessage {
            event,
            x: (msg.wParam & 0xFFFF) as i16,
            y: (msg.wParam >> 16) as i16,
        })
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

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_USER_TRAYICON {
            PostMessageW(ptr::null_mut(), Self::message(hwnd), wparam, lparam);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
