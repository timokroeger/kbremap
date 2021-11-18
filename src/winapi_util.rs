use std::ffi::OsString;
use std::os::windows::prelude::{OsStrExt, OsStringExt};
use std::{env, mem, ptr};

use wchar::wchz;
use winapi::shared::minwindef::*;
use winapi::um::winnt::*;
use winapi::um::winreg::*;

pub struct AutoStartEntry<'a> {
    key: HKEY,
    name: &'a [u16],
}

impl<'a> Drop for AutoStartEntry<'a> {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.key);
        }
    }
}

impl<'a> AutoStartEntry<'a> {
    pub fn new(name: &'a [u16]) -> Self {
        unsafe {
            let mut key: HKEY = mem::zeroed();
            RegCreateKeyW(
                HKEY_CURRENT_USER,
                wchz!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run") as _,
                &mut key,
            );
            Self { key, name }
        }
    }

    pub fn is_registered(&self) -> bool {
        let exe_path = env::current_exe().unwrap();
        unsafe {
            let mut path_buf = [0_u16; MAX_PATH];
            let mut path_len = mem::size_of_val(&path_buf) as u32;
            let key_exists = RegGetValueW(
                self.key,
                ptr::null(),
                self.name.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                &mut path_buf as *mut _ as _,
                &mut path_len as _,
            ) == 0;

            key_exists
                && OsString::from_wide(&path_buf[..(path_len as usize / 2 - 1)])
                    == exe_path.as_os_str()
        }
    }

    pub fn register(&self) {
        let mut exe_path: Vec<u16> = env::current_exe()
            .unwrap()
            .as_os_str()
            .encode_wide()
            .collect();
        exe_path.push(0);

        unsafe {
            RegSetValueExW(
                self.key,
                self.name.as_ptr(),
                0,
                REG_SZ,
                exe_path.as_mut_ptr() as _,
                (exe_path.len() * 2) as _,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueW(self.key, self.name.as_ptr());
        }
    }
}
