use std::{mem, process, ptr};

use once_cell::sync::OnceCell;
use winapi::{
    shared::{
        minwindef::{LPARAM, LRESULT, UINT, WPARAM},
        windef::{HICON, HWND},
    },
    um::{
        libloaderapi,
        shellapi::{self, NOTIFYICONDATAA},
        winuser,
    },
};

const TRAYICON_UID: UINT = 873;
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

struct TrayIconState {
    on_double_click: Option<Box<dyn FnMut()>>,
}

pub struct TrayIcon {
    hwnd: HWND,
    id: u32,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            shellapi::Shell_NotifyIconA(
                shellapi::NIM_DELETE,
                &mut Self::notification_data(self.hwnd, self.id),
            );
            Box::from_raw(Self::state_ptr(self.hwnd));
            winuser::DestroyWindow(self.hwnd);
        }
    }
}

impl TrayIcon {
    pub fn new() -> Self {
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

            // incrementing icon id
            let tray_icon_id = TRAYICON_UID;

            // Create the tray icon
            let mut notification_data = Self::notification_data(hwnd, tray_icon_id);
            notification_data.uFlags = shellapi::NIF_MESSAGE;
            notification_data.uCallbackMessage = WM_USER_TRAYICON;
            shellapi::Shell_NotifyIconA(shellapi::NIM_ADD, &mut notification_data);

            let state = Box::new(TrayIconState {
                on_double_click: None,
            });
            winuser::SetWindowLongPtrA(
                hwnd,
                winuser::GWLP_USERDATA,
                Box::leak(state) as *const _ as _,
            );

            Self {
                hwnd,
                id: tray_icon_id,
            }
        }
    }

    pub fn set_icon(&self, icon: IconResource) {
        let mut notification_data = Self::notification_data(self.hwnd, self.id);
        notification_data.uFlags = shellapi::NIF_ICON;
        notification_data.hIcon = icon.0;
        unsafe {
            shellapi::Shell_NotifyIconA(shellapi::NIM_MODIFY, &mut notification_data);
        }
    }

    pub fn on_double_click(&mut self, cb: impl FnMut() + 'static) {
        Self::state(&self.hwnd).on_double_click = Some(Box::new(cb));
    }

    fn notification_data(hwnd: HWND, id: u32) -> NOTIFYICONDATAA {
        unsafe {
            let mut notification_data: NOTIFYICONDATAA = mem::zeroed();
            notification_data.cbSize = mem::size_of_val(&notification_data) as _;
            notification_data.hWnd = hwnd;
            notification_data.uID = id;
            *notification_data.u.uVersion_mut() = shellapi::NOTIFYICON_VERSION_4;
            notification_data
        }
    }

    fn state_ptr(hwnd: HWND) -> *mut TrayIconState {
        unsafe { winuser::GetWindowLongPtrA(hwnd, winuser::GWLP_USERDATA) as _ }
    }

    fn state(hwnd: &HWND) -> &mut TrayIconState {
        unsafe { &mut *Self::state_ptr(*hwnd) }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let state = Self::state(&hwnd);

        match (msg, lparam as u32) {
            (WM_USER_TRAYICON, winuser::WM_LBUTTONDBLCLK) => {
                if let Some(cb) = &mut state.on_double_click {
                    cb();
                }
            }
            _ => return winuser::DefWindowProcA(hwnd, msg, wparam, lparam),
        }

        0
    }
}
