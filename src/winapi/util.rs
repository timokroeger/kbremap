use std::{mem, ptr};

use widestring::U16CStr;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::LibraryLoader::*;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

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
