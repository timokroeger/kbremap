use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::System::Environment::GetCommandLineA;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::ShellExecuteA;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len: u32 = 0;
        let res = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            mem::size_of_val(&elevation) as u32,
            &mut ret_len,
        );
        CloseHandle(token);

        if res == 0 {
            return false;
        }

        elevation.TokenIsElevated != 0
    }
}

pub fn elevate() -> bool {
    let cmdline = unsafe { GetCommandLineA() };

    // Copy exe name to buffer with null terminator.
    let mut param_offset = 0;
    let mut exe_buf = [0; MAX_PATH as _];
    for (offset, buf) in exe_buf.iter_mut().enumerate() {
        let c = unsafe { *cmdline.add(offset) };
        if c == 0 || c == b' ' {
            *buf = 0;
            param_offset = offset;
            break;
        }
        *buf = c;
    }

    if param_offset == 0 {
        return false;
    }

    unsafe {
        let ret = ShellExecuteA(
            ptr::null_mut(),
            c"runas".as_ptr().cast(),
            exe_buf.as_ptr(),
            cmdline.add(param_offset),
            std::ptr::null(),
            SW_NORMAL,
        );

        // Per ShellExecute docs, values > 32 indicate success.
        ret as usize > 32
    }
}
