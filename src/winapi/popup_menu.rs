use std::ffi::CStr;
use std::num::NonZeroI32;
use std::ptr;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct PopupMenu {
    hmenu: HMENU,
    id: i32,
}

impl PopupMenu {
    pub fn new() -> Self {
        let hmenu = unsafe { CreatePopupMenu() };
        debug_assert_ne!(hmenu, 0);
        Self { hmenu, id: 0 }
    }

    pub fn add_entry(&mut self, text: &CStr, checked: bool, disabled: bool) -> MenuEntry {
        self.id += 1;
        let mut flags = 0;
        if checked {
            flags |= MF_CHECKED;
        }
        if disabled {
            flags |= MF_DISABLED;
        }
        let result = unsafe {
            AppendMenuA(
                self.hmenu,
                flags,
                self.id as u32 as usize,
                text.as_ptr().cast(),
            )
        };
        debug_assert_ne!(result, 0);
        MenuEntry(NonZeroI32::new(self.id).unwrap())
    }

    pub fn show(&self, hwnd: HWND, pt: POINT) -> Option<MenuEntry> {
        unsafe {
            // Required for the menu to disappear when it loses focus.
            SetForegroundWindow(hwnd);
            let id = TrackPopupMenuEx(
                self.hmenu,
                TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
                pt.x,
                pt.y,
                hwnd,
                ptr::null(),
            );
            let id = NonZeroI32::new(id)?;
            Some(MenuEntry(id))
        }
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.hmenu) };
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MenuEntry(NonZeroI32);
