use std::{ffi::CString, mem, ptr};

use cstr::cstr;
use windows_sys::Win32::{Foundation::MAX_PATH, System::Registry::*};

pub struct AutoStartEntry {
    key: HKEY,
    name: CString,
    cmd: CString,
}

impl Drop for AutoStartEntry {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.key);
        }
    }
}

impl AutoStartEntry {
    pub fn new(name: CString, cmd: CString) -> Self {
        unsafe {
            let mut key: HKEY = mem::zeroed();
            RegCreateKeyA(
                HKEY_CURRENT_USER,
                cstr!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run")
                    .as_ptr()
                    .cast(),
                &mut key,
            );
            Self { key, name, cmd }
        }
    }

    pub fn is_registered(&self) -> bool {
        unsafe {
            let mut path_buf = [0_u8; MAX_PATH as usize];
            let mut path_len = mem::size_of_val(&path_buf) as u32;
            let key_exists = RegGetValueA(
                self.key,
                ptr::null(),
                self.name.as_ptr().cast(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::addr_of_mut!(path_buf).cast(),
                &mut path_len,
            ) == 0;

            if !key_exists {
                return false;
            }

            return &path_buf[..path_len as usize] == self.cmd.as_bytes_with_nul();
        }
    }

    pub fn register(&self) {
        unsafe {
            RegSetValueExA(
                self.key,
                self.name.as_ptr().cast(),
                0,
                REG_SZ,
                self.cmd.as_ptr().cast(),
                self.cmd.as_bytes_with_nul().len() as u32,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueA(self.key, self.name.as_ptr().cast());
        }
    }
}
