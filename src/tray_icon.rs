use std::{mem, ptr};

use once_cell::sync::OnceCell;
use wchar::wchz;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::*;
use winapi::um::shellapi::*;
use winapi::um::winuser::*;

const WM_USER_TRAYICON: UINT = WM_USER + 873;

pub enum Event {
    DoubleClick,
}

pub struct TrayIcon {
    hwnd: HWND,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &mut Self::notification_data(self.hwnd));
            DestroyWindow(self.hwnd);
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

            static WINDOW_CLASS: OnceCell<u16> = OnceCell::new();
            let wnd_class_atom = *WINDOW_CLASS.get_or_init(|| {
                let mut wnd_class: WNDCLASSW = mem::zeroed();
                wnd_class.lpfnWndProc = Some(Self::wndproc);
                wnd_class.hInstance = hinstance;
                wnd_class.lpszClassName = wchz!("trayicon").as_ptr();
                let wnd_class_atom = RegisterClassW(&wnd_class);
                assert_ne!(wnd_class_atom, 0);
                wnd_class_atom
            });

            // Create a message only window to receive tray icon mouse events.
            // <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows>
            let hwnd = CreateWindowExW(
                0,
                wnd_class_atom as _,
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

            // Set message as associated data
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, message as _);

            // Create the tray icon
            let mut notification_data = Self::notification_data(hwnd);
            notification_data.uFlags = NIF_MESSAGE;
            notification_data.uCallbackMessage = WM_USER_TRAYICON;
            Shell_NotifyIconW(NIM_ADD, &mut notification_data);

            Self { hwnd }
        }
    }

    pub fn set_icon(&self, icon: HICON) {
        let mut notification_data = Self::notification_data(self.hwnd);
        notification_data.uFlags = NIF_ICON;
        notification_data.hIcon = icon;
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut notification_data);
        }
    }

    pub fn event_from_message(&self, msg: &MSG) -> Option<Event> {
        if msg.message != Self::message(self.hwnd) {
            return None;
        }

        match msg.lParam as _ {
            WM_LBUTTONDBLCLK => Some(Event::DoubleClick),
            _ => None,
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
