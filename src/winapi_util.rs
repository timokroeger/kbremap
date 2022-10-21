use std::{mem, ptr};

use widestring::{u16cstr, U16CStr, U16CString};
use winapi::shared::minwindef::*;
use winapi::um::winnt::*;
use winapi::um::winreg::*;

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
            let mut path_buf = [0_u16; MAX_PATH];
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
