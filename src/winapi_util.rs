use std::{mem, ptr};

use widestring::{u16cstr, U16CStr, U16CString};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::CreateMutexW;

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

pub fn disable_quick_edit_mode() {
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
