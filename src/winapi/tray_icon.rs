use std::marker::PhantomData;
use std::{mem, ptr};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const MSG_ID_TRAY_ICON: u32 = WM_USER;

struct State {
    msg_id: u32,
    icon: HICON,
}

impl State {
    fn raw_from_hwnd(hwnd: HWND) -> *mut Self {
        let ptr = unsafe { GetWindowLongPtrA(hwnd, GWLP_USERDATA) as *mut Self };
        assert!(!ptr.is_null());
        ptr
    }

    fn from_hwnd(hwnd: HWND) -> &'static mut Self {
        unsafe { &mut *Self::raw_from_hwnd(hwnd) }
    }
}

pub struct TrayIcon {
    hwnd: HWND,
    _state: PhantomData<*mut State>,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let state_ptr = State::raw_from_hwnd(self.hwnd);
        unsafe {
            Shell_NotifyIconA(NIM_DELETE, &notification_data(self.hwnd));
            DestroyWindow(self.hwnd);
            drop(Box::from_raw(state_ptr));
        }
    }
}

impl TrayIcon {
    pub fn new(msg_id: u32, icon: HICON) -> Self {
        assert!(
            (WM_APP..WM_APP + 0x4000).contains(&msg_id),
            "message must be in the WM_APP range"
        );

        let state = Box::new(State { msg_id, icon });

        let class_name = "trayicon\0".as_ptr();
        let hwnd = unsafe {
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
            CreateWindowExA(
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
                Box::into_raw(state) as *mut _,
            )
        };
        assert_ne!(hwnd, 0);

        // Create the tray icon
        add_tray_icon(hwnd, icon);

        Self {
            hwnd,
            _state: PhantomData,
        }
    }

    pub fn set_icon(&mut self, icon: HICON) {
        update_tray_icon(self.hwnd, icon);
        State::from_hwnd(self.hwnd).icon = icon;
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    println!(
        "WND  hwnd={:p} msg={:06}, wparam={:06}, lparam={:06}",
        hwnd as *const (), msg, wparam, lparam
    );

    if msg == WM_NCCREATE {
        // Attach user data to the window so it can be accessed from this
        // callback function when receiving other messages.
        // This must be done here because the WM_NCCREATE (which is the very
        // first message of each window) and other message are dispatched to
        // this callback before `CreateWindowEx()` returns.
        // https://devblogs.microsoft.com/oldnewthing/20191014-00/?p=102992
        let create_params = lparam as *const CREATESTRUCTA;
        SetWindowLongPtrA(
            hwnd,
            GWLP_USERDATA,
            (*create_params).lpCreateParams as isize,
        );
        return DefWindowProcA(hwnd, msg, wparam, lparam);
    }

    let state = State::from_hwnd(hwnd);
    if msg == MSG_ID_TRAY_ICON {
        // Forward the message to the main event loop.
        PostMessageA(hwnd, state.msg_id, wparam, lparam);
    } else if msg == RegisterWindowMessageA("TaskbarCreated\0".as_ptr()) {
        // Re-add the tray icon if explorer.exe has restarted.
        add_tray_icon(hwnd, state.icon);
    }

    DefWindowProcA(hwnd, msg, wparam, lparam)
}

fn add_tray_icon(hwnd: HWND, icon: HICON) {
    let mut notification_data = notification_data(hwnd);
    notification_data.uFlags = NIF_MESSAGE | NIF_ICON;
    notification_data.uCallbackMessage = MSG_ID_TRAY_ICON;
    notification_data.hIcon = icon;
    unsafe { Shell_NotifyIconA(NIM_ADD, &notification_data) };
}

fn update_tray_icon(hwnd: HWND, icon: HICON) {
    let mut notification_data = notification_data(hwnd);
    notification_data.uFlags = NIF_ICON;
    notification_data.hIcon = icon;
    unsafe { Shell_NotifyIconA(NIM_MODIFY, &notification_data) };
}

fn notification_data(hwnd: HWND) -> NOTIFYICONDATAA {
    let mut notification_data: NOTIFYICONDATAA = unsafe { mem::zeroed() };
    notification_data.cbSize = mem::size_of_val(&notification_data) as _;
    notification_data.hWnd = hwnd;
    notification_data
}
