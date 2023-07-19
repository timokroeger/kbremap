use std::ptr;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct Icon(pub HICON);

impl Icon {
    pub fn from_rc_numeric(id: u16) -> Self {
        let hicon =
            unsafe { LoadImageA(GetModuleHandleA(ptr::null()), id as _, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE) };
        assert_ne!(hicon, 0, "icon resource {} not found", id);
        Self(hicon)
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        unsafe { DestroyIcon(self.0) };
    }
}
