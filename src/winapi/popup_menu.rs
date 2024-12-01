use std::ffi::CStr;
use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winmsg_executor::util::Window;

pub struct PopupMenu(HMENU);

impl PopupMenu {
    pub fn new() -> Self {
        let hmenu = unsafe { CreatePopupMenu() };
        assert!(!hmenu.is_null());
        Self(hmenu)
    }

    pub fn add_entry(&self, id: u32, flags: u32, text: &CStr) {
        assert_ne!(id, 0, "menu entry id cannot be zero");
        let result = unsafe { AppendMenuA(self.0, flags, id as usize, text.as_ptr().cast()) };
        assert_ne!(result, 0);
    }

    pub fn show(&self, pt: POINT) -> Option<u32> {
        // TrackPopupMenuEx requires a window handle to work even though it doesn't use it.
        let dummy_window = Window::new_reentrant(true, (), |_, _| None).unwrap();
        unsafe {
            // Required for the menu to disappear when it loses focus.
            SetForegroundWindow(dummy_window.hwnd());
            let id = TrackPopupMenuEx(
                self.0,
                TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
                pt.x,
                pt.y,
                dummy_window.hwnd(),
                ptr::null(),
            );
            if id == 0 {
                None
            } else {
                Some(id as u32)
            }
        }
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.0) };
    }
}
