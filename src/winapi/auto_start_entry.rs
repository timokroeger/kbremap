use std::{ffi::CStr, mem, ptr};

use windows_sys::Win32::{
    Foundation::MAX_PATH,
    System::{LibraryLoader::GetModuleFileNameA, Registry::*},
};

pub struct AutoStartEntry<'a> {
    key: HKEY,
    name: &'a CStr,
}

impl Drop for AutoStartEntry<'_> {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.key);
        }
    }
}

impl<'a> AutoStartEntry<'a> {
    pub fn new(name: &'a CStr) -> Self {
        unsafe {
            let mut key: HKEY = mem::zeroed();
            RegCreateKeyA(
                HKEY_CURRENT_USER,
                c"SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"
                    .as_ptr()
                    .cast(),
                &mut key,
            );
            Self { key, name }
        }
    }

    pub fn is_registered(&self) -> bool {
        unsafe {
            let mut path_buf_reg = [0_u8; MAX_PATH as usize];
            let mut path_len_reg = mem::size_of_val(&path_buf_reg) as u32;
            let key_exists = RegGetValueA(
                self.key,
                ptr::null(),
                self.name.as_ptr().cast(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::addr_of_mut!(path_buf_reg).cast(),
                &mut path_len_reg,
            ) == 0;

            if !key_exists {
                return false;
            }

            let mut path_buf_exe = [0_u8; MAX_PATH as usize];
            let mut path_len_exe = GetModuleFileNameA(
                ptr::null_mut(),
                path_buf_exe.as_mut_ptr(),
                path_buf_exe.len() as _,
            ) as usize;
            if path_len_exe == 0 || path_len_exe == path_buf_exe.len() {
                return false;
            }

            // Add 1 to the length of the executable path to include the null
            // terminator in the string comparison.
            path_len_exe += 1;
            return &path_buf_exe[..path_len_exe] == &path_buf_reg[..path_len_reg as usize];
        }
    }

    pub fn register(&self) {
        unsafe {
            let mut path_buf_exe = [0_u8; MAX_PATH as usize];
            let mut path_len_exe = GetModuleFileNameA(
                ptr::null_mut(),
                path_buf_exe.as_mut_ptr(),
                path_buf_exe.len() as u32,
            ) as usize;
            if path_len_exe == 0 || path_len_exe == path_buf_exe.len() {
                return;
            }

            // Add 1 to the length of the executable path to include the null
            // terminator as required by the `RegSetValueEx()` function.
            path_len_exe += 1;

            RegSetValueExA(
                self.key,
                self.name.as_ptr().cast(),
                0,
                REG_SZ,
                path_buf_exe[..path_len_exe].as_ptr().cast(),
                path_len_exe as _,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueA(self.key, self.name.as_ptr().cast());
        }
    }
}
