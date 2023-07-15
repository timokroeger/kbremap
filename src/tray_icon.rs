use std::{mem, ptr};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct UserData {
    msg_tray: u32,
    icon: HICON,
}

pub struct TrayIcon {
    hwnd: HWND,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(userdata(self.hwnd)));
            Shell_NotifyIconA(NIM_DELETE, &notification_data(self.hwnd));
            DestroyWindow(self.hwnd);
        }
    }
}

impl TrayIcon {
    pub fn new(msg_tray: u32, icon: HICON) -> Self {
        assert!(
            (WM_APP..WM_APP + 0x4000).contains(&msg_tray),
            "message must be in the WM_APP range"
        );

        let class_name = "trayicon\0".as_ptr();
        unsafe {
            let hinstance = GetModuleHandleA(ptr::null());

            // A class is unique and the `RegisterClass()` function fails when
            // we create more than one tray icon but we do not care.
            let mut wnd_class: WNDCLASSA = mem::zeroed();
            wnd_class.lpfnWndProc = Some(wndproc);
            wnd_class.hInstance = hinstance;
            wnd_class.lpszClassName = class_name;
            RegisterClassA(&wnd_class);

            // Never visible, only required to receive messages.
            // It cannot be a message-only `HWND_MESSAGE` thought because those do not
            // receive global messages like "TaskbarCreated".
            let hwnd = CreateWindowExA(
                0,
                class_name,
                ptr::null(),
                WS_POPUP, // Not visible anyway, might use less resources.
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                ptr::null(),
            );
            assert_ne!(hwnd, 0);

            // Set message as associated data
            let userdata = Box::new(UserData { msg_tray, icon });
            SetWindowLongPtrA(hwnd, GWLP_USERDATA, Box::into_raw(userdata) as isize);

            // Create the tray icon
            add_tray_icon(hwnd);

            Self { hwnd }
        }
    }

    pub fn set_icon(&self, icon: HICON) {
        userdata(self.hwnd).icon = icon;
        let mut notification_data = notification_data(self.hwnd);
        notification_data.uFlags = NIF_ICON;
        notification_data.hIcon = icon;
        unsafe {
            Shell_NotifyIconA(NIM_MODIFY, &notification_data);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_USER => {
            // Forward the message to the main event loop.
            PostMessageA(hwnd, userdata(hwnd).msg_tray, wparam, lparam);
        }
        msg if msg == RegisterWindowMessageA("TaskbarCreated\0".as_ptr()) => {
            // Re-add the tray icon if explorer.exe has restarted.
            add_tray_icon(hwnd);
        }
        msg => return DefWindowProcA(hwnd, msg, wparam, lparam),
    }
    0
}

fn add_tray_icon(hwnd: HWND) {
    let mut notification_data = notification_data(hwnd);
    notification_data.uFlags = NIF_MESSAGE | NIF_ICON;
    notification_data.uCallbackMessage = WM_USER;
    notification_data.hIcon = userdata(hwnd).icon;
    unsafe { Shell_NotifyIconA(NIM_ADD, &mut notification_data) };
}

fn notification_data(hwnd: HWND) -> NOTIFYICONDATAA {
    unsafe {
        let mut notification_data: NOTIFYICONDATAA = mem::zeroed();
        notification_data.cbSize = mem::size_of_val(&notification_data) as _;
        notification_data.hWnd = hwnd;
        notification_data
    }
}

fn userdata(hwnd: HWND) -> &'static mut UserData {
    unsafe {
        let userdata = GetWindowLongPtrA(hwnd, GWLP_USERDATA) as *mut UserData;
        &mut *userdata
    }
}
