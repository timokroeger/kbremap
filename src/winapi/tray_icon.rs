use std::cell::Cell;
use std::mem;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winmsg_executor::util::Window;

use crate::winapi::StaticIcon;

const MSG_ID_TRAY_ICON: u32 = WM_USER;

struct State {
    icon: Cell<StaticIcon>,
}

pub struct TrayIcon {
    window: Window<State>,
}

impl TrayIcon {
    pub fn new(msg_id: u32, icon: StaticIcon) -> Self {
        assert!(
            (WM_APP..WM_APP + 0x4000).contains(&msg_id),
            "message must be in the WM_APP range"
        );

        let msg_id_taskbar_created =
            unsafe { RegisterWindowMessageA(c"TaskbarCreated".as_ptr() as *const u8) };
        let window = Window::new_reentrant(
            false,
            State {
                icon: Cell::new(icon),
            },
            move |state, msg| {
                if msg.msg == MSG_ID_TRAY_ICON {
                    // Forward the message to the main event loop.
                    unsafe { PostMessageA(msg.hwnd, msg_id, msg.wparam, msg.lparam) };
                    return Some(0);
                }

                if msg.msg == msg_id_taskbar_created {
                    // Re-add the tray icon if explorer.exe has restarted.
                    add_tray_icon(msg.hwnd, state.icon.get());
                    return Some(0);
                }

                None
            },
        )
        .unwrap();

        // Create the tray icon
        add_tray_icon(window.hwnd(), icon);

        Self { window }
    }

    pub fn set_icon(&self, icon: StaticIcon) {
        update_tray_icon(self.window.hwnd(), icon);
        self.window.shared_state().icon.set(icon);
    }
}

fn add_tray_icon(hwnd: HWND, icon: StaticIcon) {
    let mut notification_data = notification_data(hwnd);
    notification_data.uFlags = NIF_MESSAGE | NIF_ICON;
    notification_data.uCallbackMessage = MSG_ID_TRAY_ICON;
    notification_data.hIcon = icon.handle();
    notification_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    unsafe {
        Shell_NotifyIconA(NIM_ADD, &notification_data);
        Shell_NotifyIconA(NIM_SETVERSION, &notification_data);
    }
}

fn update_tray_icon(hwnd: HWND, icon: StaticIcon) {
    let mut notification_data = notification_data(hwnd);
    notification_data.uFlags = NIF_ICON;
    notification_data.hIcon = icon.handle();
    unsafe { Shell_NotifyIconA(NIM_MODIFY, &notification_data) };
}

fn notification_data(hwnd: HWND) -> NOTIFYICONDATAA {
    let mut notification_data: NOTIFYICONDATAA = unsafe { mem::zeroed() };
    notification_data.cbSize = mem::size_of_val(&notification_data) as _;
    notification_data.hWnd = hwnd;
    notification_data
}
