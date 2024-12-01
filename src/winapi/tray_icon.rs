use std::cell::Cell;
use std::mem;

use pin_project::pin_project;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winmsg_executor::util::Window;

use crate::util::Notification;
use crate::winapi::StaticIcon;

const MSG_ID_TRAY_ICON: u32 = WM_USER;

#[pin_project]
struct State {
    icon: Cell<StaticIcon>,
    #[pin]
    double_click: Notification<POINT>,
    #[pin]
    right_click: Notification<POINT>,
}

pub struct TrayIcon {
    window: Window<State>,
}

impl TrayIcon {
    pub fn new(icon: StaticIcon) -> Self {
        let msg_id_taskbar_created =
            unsafe { RegisterWindowMessageA(c"TaskbarCreated".as_ptr() as *const u8) };
        let window = Window::new_reentrant(
            false,
            State {
                icon: Cell::new(icon),
                double_click: Notification::new(POINT { x: 0, y: 0 }),
                right_click: Notification::new(POINT { x: 0, y: 0 }),
            },
            move |state, msg| {
                let state = state.project_ref();

                if msg.msg == MSG_ID_TRAY_ICON {
                    let pt = POINT {
                        x: (msg.wparam as i16).into(),
                        y: ((msg.wparam >> 16) as i16).into(),
                    };
                    match msg.lparam as u32 {
                        WM_LBUTTONDBLCLK => {
                            state.double_click.send(pt);
                        }
                        WM_RBUTTONUP => {
                            state.right_click.send(pt);
                        }
                        _ => {}
                    }
                    Some(0)
                } else if msg.msg == msg_id_taskbar_created {
                    // Re-add the tray icon if explorer.exe has restarted.
                    add_tray_icon(msg.hwnd, state.icon.get());
                    Some(0)
                } else {
                    None // Message not handled
                }
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

    pub async fn double_click(&self) -> POINT {
        let state = self.window.shared_state().project_ref();
        state.double_click.receive().await
    }

    pub async fn right_click(&self) -> POINT {
        let state = self.window.shared_state().project_ref();
        state.right_click.receive().await
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
