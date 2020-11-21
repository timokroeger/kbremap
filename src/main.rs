mod keyboard_hook;

use std::mem;

use keyboard_hook::KeyboardHook;
use winapi::{shared::windef::*, um::winuser::*};

fn main() {
    KeyboardHook::set(|scan_code, up| true);

    unsafe {
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&mut msg);
            DispatchMessageW(&mut msg);
        }
    }
}
