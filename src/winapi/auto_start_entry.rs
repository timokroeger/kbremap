use std::{ffi::CStr, mem, ptr};

use windows_sys::Win32::{
    Foundation::MAX_PATH,
    System::{
        Registry::*,
        Threading::{GetCurrentProcess, QueryFullProcessImageNameA},
    },
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
                &raw mut key,
            );
            Self { key, name }
        }
    }

    pub fn is_registered(&self) -> bool {
        let mut path_buf_reg = [0_u8; MAX_PATH as usize];
        let mut path_len_reg = mem::size_of_val(&path_buf_reg) as u32;
        let key_exists = unsafe {
            RegGetValueA(
                self.key,
                ptr::null(),
                self.name.as_ptr().cast(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                path_buf_reg.as_mut_ptr().cast(),
                &raw mut path_len_reg,
            )
        } == 0;
        if !key_exists {
            return false;
        }

        let mut path_buf_exe = [0_u8; MAX_PATH as usize];
        let path_buf_exe = exe_path(&mut path_buf_exe);
        if path_buf_exe.is_empty() {
            return false;
        }

        path_buf_exe == &path_buf_reg[..path_len_reg as usize]
    }

    pub fn register(&self) {
        let mut path_buf_exe = [0_u8; MAX_PATH as usize];
        let path_buf_exe = exe_path(&mut path_buf_exe);
        if path_buf_exe.is_empty() {
            return;
        }

        unsafe {
            RegSetValueExA(
                self.key,
                self.name.as_ptr().cast(),
                0,
                REG_SZ,
                path_buf_exe.as_ptr().cast(),
                path_buf_exe.len() as _,
            )
        };
    }

    pub fn remove(&self) {
        unsafe {
            RegDeleteValueA(self.key, self.name.as_ptr().cast());
        }
    }
}

// Returns the path to the current executable as a byte slice including the null
// terminator. Returns a zero length slice on failure.
fn exe_path(path_buf: &mut [u8]) -> &[u8] {
    let mut buf_len = path_buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameA(GetCurrentProcess(), 0, path_buf.as_mut_ptr(), &raw mut buf_len)
    };
    if ok == 0 {
        return &[];
    }
    let len = buf_len as usize + 1; // Include null terminator
    if len > path_buf.len() {
        return &[];
    }
    &path_buf[..len]
}
