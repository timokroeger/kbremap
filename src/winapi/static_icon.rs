use std::ptr;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Copy)]
pub struct StaticIcon(HICON);

impl StaticIcon {
    pub fn from_rc_numeric(id: u16) -> Self {
        let hicon = unsafe {
            LoadImageA(
                GetModuleHandleA(ptr::null()),
                id as _,
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            )
        };
        assert!(!hicon.is_null(), "icon resource {} not found", id);
        Self(hicon)
    }

    pub fn handle(&self) -> HICON {
        self.0
    }
}
