use std::{mem, ptr};

use once_cell::sync::OnceCell;
use winapi::{
    shared::{
        minwindef::{LPARAM, LRESULT, UINT, WPARAM},
        windef::{HICON, HWND},
    },
    um::{
        libloaderapi,
        shellapi::{self, NOTIFYICONDATAA},
        winuser::{self, MSG},
    },
};

const WM_USER_TRAYICON: UINT = winuser::WM_USER + 873;

#[derive(Clone, Copy)]
pub struct IconResource(HICON);

impl IconResource {
    pub fn load_numeric_id(id: u16) -> Self {
        unsafe {
            Self(winuser::LoadIconA(
                libloaderapi::GetModuleHandleA(ptr::null()),
                id as _,
            ))
        }
    }
}

pub enum Event {
    DoubleClick,
}

pub struct TrayIcon {
    hwnd: HWND,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            shellapi::Shell_NotifyIconA(
                shellapi::NIM_DELETE,
                &mut Self::notification_data(self.hwnd),
            );
            winuser::DestroyWindow(self.hwnd);
        }
    }
}

impl TrayIcon {
    pub fn new(message: u32) -> Self {
        assert!(
            message >= winuser::WM_APP && message < winuser::WM_APP + 0x4000,
            "message must be in the WM_APP range"
        );

        unsafe {
            let hinstance = libloaderapi::GetModuleHandleA(ptr::null());

            static WINDOW_CLASS: OnceCell<u16> = OnceCell::new();
            let wnd_class_atom = *WINDOW_CLASS.get_or_init(|| {
                let mut wnd_class: winuser::WNDCLASSA = mem::zeroed();
                wnd_class.lpfnWndProc = Some(Self::wndproc);
                wnd_class.hInstance = hinstance;
                wnd_class.lpszClassName = b"trayicon\0" as *const _ as _;
                let wnd_class_atom = winuser::RegisterClassA(&wnd_class);
                assert_ne!(wnd_class_atom, 0);
                wnd_class_atom
            });

            // Create a message only window to receive tray icon mouse events.
            // <https://docs.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows>
            let hwnd = winuser::CreateWindowExA(
                0,
                wnd_class_atom as _,
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                winuser::HWND_MESSAGE,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            assert_ne!(hwnd, ptr::null_mut());

            // Set message as associated data
            winuser::SetWindowLongPtrA(hwnd, winuser::GWLP_USERDATA, message as _);

            // Create the tray icon
            let mut notification_data = Self::notification_data(hwnd);
            notification_data.uFlags = shellapi::NIF_MESSAGE;
            notification_data.uCallbackMessage = WM_USER_TRAYICON;
            shellapi::Shell_NotifyIconA(shellapi::NIM_ADD, &mut notification_data);

            Self { hwnd }
        }
    }

    pub fn set_icon(&self, icon: IconResource) {
        let mut notification_data = Self::notification_data(self.hwnd);
        notification_data.uFlags = shellapi::NIF_ICON;
        notification_data.hIcon = icon.0;
        unsafe {
            shellapi::Shell_NotifyIconA(shellapi::NIM_MODIFY, &mut notification_data);
        }
    }

    pub fn event_from_message(&self, msg: &MSG) -> Option<Event> {
        if msg.message != Self::message(self.hwnd) {
            return None;
        }

        match msg.lParam as _ {
            winuser::WM_LBUTTONDBLCLK => Some(Event::DoubleClick),
            _ => None,
        }
    }

    fn notification_data(hwnd: HWND) -> NOTIFYICONDATAA {
        unsafe {
            let mut notification_data: NOTIFYICONDATAA = mem::zeroed();
            notification_data.cbSize = mem::size_of_val(&notification_data) as _;
            notification_data.hWnd = hwnd;
            notification_data.uID = Self::message(hwnd);
            *notification_data.u.uVersion_mut() = shellapi::NOTIFYICON_VERSION_4;
            notification_data
        }
    }

    fn message(hwnd: HWND) -> u32 {
        unsafe { winuser::GetWindowLongPtrA(hwnd, winuser::GWLP_USERDATA) as _ }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_USER_TRAYICON {
            winuser::PostMessageA(ptr::null_mut(), Self::message(hwnd), wparam, lparam);
            return 0;
        }
        winuser::DefWindowProcA(hwnd, msg, wparam, lparam)
    }
}
