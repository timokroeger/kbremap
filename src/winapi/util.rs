use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

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
            c"CONIN$".as_ptr().cast(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        );
        let mut mode: u32 = 0;
        if GetConsoleMode(console.cast(), &raw mut mode) != 0 {
            mode &= !ENABLE_QUICK_EDIT_MODE;
            mode |= ENABLE_EXTENDED_FLAGS;
            SetConsoleMode(console.cast(), mode);
        }
        CloseHandle(console);
    }
}
