use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::{mem, ptr};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winmsg_executor::util::{Window, WindowMessage, WindowType};

use crate::winapi::StaticIcon;

const MSG_ID_TRAY_ICON: u32 = WM_USER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconEvent {
    Click,
    DoubleClick,
    MenuItem(u32),
}

pub struct Menu(HMENU);

impl Menu {
    fn new() -> Self {
        let hmenu = unsafe { CreatePopupMenu() };
        assert!(!hmenu.is_null());
        Self(hmenu)
    }

    pub fn add_entry(&mut self, id: u32, flags: u32, text: &CStr) {
        assert_ne!(id, 0, "menu entry id cannot be zero");
        let result = unsafe { AppendMenuA(self.0, flags, id as usize, text.as_ptr().cast()) };
        assert_ne!(result, 0);
    }

    fn show(&mut self, hwnd: HWND, x: i32, y: i32) -> Option<u32> {
        unsafe {
            // Required for the menu to disappear when it loses focus.
            SetForegroundWindow(hwnd);
            let id = TrackPopupMenuEx(
                self.0,
                TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
                x,
                y,
                hwnd,
                ptr::null(),
            );
            if id == 0 {
                None
            } else {
                Some(id as u32)
            }
        }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.0) };
    }
}

struct Handlers {
    event: Option<Box<dyn FnMut(TrayIconEvent)>>,
    #[allow(clippy::type_complexity)]
    contex_menu: Option<Box<dyn FnMut(&mut Menu)>>,
}

struct State {
    icon: Cell<StaticIcon>,
    handlers: RefCell<Handlers>,
}

pub struct TrayIcon {
    window: Window<State>,
}

impl TrayIcon {
    pub fn new(icon: StaticIcon) -> Self {
        let msg_id_taskbar_created =
            unsafe { RegisterWindowMessageA(c"TaskbarCreated".as_ptr() as *const u8) };
        let window = Window::new(
            WindowType::MessageOnly,
            State {
                icon: Cell::new(icon),
                handlers: RefCell::new(Handlers {
                    event: None,
                    contex_menu: None,
                }),
            },
            move |state, msg| {
                if msg.msg == MSG_ID_TRAY_ICON {
                    handle_tray_icon_event(&state.handlers, msg);
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

    pub fn on_event(&self, handler: impl FnMut(TrayIconEvent) + 'static) {
        self.window.state().handlers.borrow_mut().event = Some(Box::new(handler));
    }

    pub fn on_menu(&self, handler: impl FnMut(&mut Menu) + 'static) {
        self.window.state().handlers.borrow_mut().contex_menu = Some(Box::new(handler));
    }

    pub fn set_icon(&self, icon: StaticIcon) {
        update_tray_icon(self.window.hwnd(), icon);
        self.window.state().icon.set(icon);
    }
}

fn handle_tray_icon_event(handlers: &RefCell<Handlers>, msg: WindowMessage) {
    // Event is in the lower half, icon ID in the upper half.
    // Our icon ID is 0 anyway but mask it away in case it changes in future.
    let event_msg = (msg.lparam & 0xFFFF) as u32;
    let tray_icon_event = match event_msg {
        WM_LBUTTONUP => TrayIconEvent::Click,
        WM_LBUTTONDBLCLK => TrayIconEvent::DoubleClick,
        WM_CONTEXTMENU => {
            // Show context menu if registered.
            let mut handlers = handlers.borrow_mut();
            let Some(context_menu_handler) = handlers.contex_menu.as_mut() else {
                return;
            };

            let mut menu = Menu::new();
            context_menu_handler(&mut menu);

            let Some(id) = menu.show(
                msg.hwnd,
                (msg.wparam & 0xFFFF) as _,
                ((msg.wparam >> 16) & 0xFFFF) as _,
            ) else {
                return;
            };

            TrayIconEvent::MenuItem(id)
        }
        _ => return,
    };

    if let Some(ref mut event_handler) = handlers.borrow_mut().event {
        event_handler(tray_icon_event);
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
