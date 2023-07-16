use std::{mem, ptr};

use widestring::{u16cstr, U16CStr, U16CString};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct AutoStartEntry {
    key: HKEY,
    name: U16CString,
    cmd: U16CString,
}

impl Drop for AutoStartEntry {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.key);
        }
    }
}

impl AutoStartEntry {
    pub fn new(name: U16CString, cmd: U16CString) -> Self {
        unsafe {
            let mut key: HKEY = mem::zeroed();
            RegCreateKeyW(
                HKEY_CURRENT_USER,
                u16cstr!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run").as_ptr(),
                &mut key,
            );
            Self { key, name, cmd }
        }
    }

    pub fn is_registered(&self) -> bool {
        unsafe {
            let mut path_buf = [0_u16; MAX_PATH as usize];
            let mut path_len = mem::size_of_val(&path_buf) as u32;
            let key_exists = RegGetValueW(
                self.key,
                ptr::null(),
                self.name.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::addr_of_mut!(path_buf).cast(),
                &mut path_len as _,
            ) == 0;

            if !key_exists {
                return false;
            }

            if let Ok(value) = U16CStr::from_slice(&path_buf[..(path_len / 2) as usize]) {
                return value == self.cmd;
            }

            false
        }
    }

    pub fn register(&self) {
        unsafe {
            RegSetValueExW(
                self.key,
                self.name.as_ptr(),
                0,
                REG_SZ,
                self.cmd.as_ptr().cast(),
                ((self.cmd.len() + 1) * 2) as _,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueW(self.key, self.name.as_ptr());
        }
    }
}

// Returns true when this process is the first instance with the given name.
pub fn register_instance(name: &U16CStr) -> bool {
    unsafe {
        let handle = CreateMutexW(ptr::null(), 0, name.as_ptr());
        assert_ne!(handle, 0);

        if GetLastError() != ERROR_ALREADY_EXISTS {
            // Intentionally leak the mutex object to protect this instance
            // of the process.
            return true;
        }

        CloseHandle(handle);
        false
    }
}

pub fn icon_from_rc_numeric(id: u16) -> HICON {
    let hicon = unsafe { LoadImageW(GetModuleHandleW(ptr::null()), id as _, IMAGE_ICON, 0, 0, 0) };
    assert_ne!(hicon, 0, "icon resource {} not found", id);
    hicon
}

pub fn popupmenu_from_rc_numeric(id: u16) -> HMENU {
    unsafe {
        let menu = LoadMenuA(GetModuleHandleA(ptr::null()), id as _);
        assert_ne!(menu, 0, "menu resource {} not found", id);
        let submenu = GetSubMenu(menu, 0);
        assert_ne!(
            submenu, 0,
            "menu resource {} requires a popup submenu item",
            id
        );
        submenu
    }
}

pub fn message_loop(mut cb: impl FnMut(&MSG)) -> i32 {
    unsafe {
        let mut msg = mem::zeroed();
        loop {
            match GetMessageA(&mut msg, 0, 0, 0) {
                1 => {
                    cb(&msg);
                    TranslateMessage(&msg);
                    DispatchMessageA(&msg);
                }
                0 => return msg.wParam as _,
                _ => unreachable!(),
            }
        }
    }
}

// Attaches to the terminal when running from command line.
// Returns true when a terminal to print stdout is available.
pub fn console_check() -> bool {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 || GetLastError() != ERROR_INVALID_HANDLE }
}

// Opens a console window without close button and quick edit mode disabled.
// The close button is disabled to prevent the user from accidentally killing the process.
pub fn console_open() {
    unsafe {
        AllocConsole();
        let console = GetConsoleWindow();
        let console_menu = GetSystemMenu(console, 0);
        DeleteMenu(console_menu, SC_CLOSE as _, MF_BYCOMMAND);
    }
    disable_quick_edit_mode();
}

pub fn console_close() {
    unsafe { FreeConsole() };
}

// Quick edit mode is a windows feature that allows users to select text in the terminal.
// Unfortunately it is easily triggered by a single click anywhere in the console window
// and halts the whole process.
fn disable_quick_edit_mode() {
    unsafe {
        let console = CreateFileA(
            "CONIN$\0".as_ptr() as _,
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            0,
        );
        let mut mode: u32 = 0;
        if GetConsoleMode(console as _, &mut mode) != 0 {
            mode &= !ENABLE_QUICK_EDIT_MODE;
            mode |= ENABLE_EXTENDED_FLAGS;
            SetConsoleMode(console as _, mode);
        }
        CloseHandle(console);
    }
}

pub fn popup_menu(menu: HMENU, msg: &MSG) -> i32 {
    unsafe {
        // Required for the menu to disappear when it loses focus.
        SetForegroundWindow(msg.hwnd);
        TrackPopupMenuEx(
            menu,
            TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
            msg.pt.x,
            msg.pt.y,
            msg.hwnd,
            ptr::null(),
        )
    }
}
