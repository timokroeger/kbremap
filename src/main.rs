mod keyboard_hook;

use std::{collections::HashMap, ffi::OsStr, mem, os::windows::ffi::OsStrExt};

use keyboard_hook::KeyboardHook;
use winapi::{shared::windef::*, um::winuser::*};

fn send_unicode(up: bool, c: u16) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wScan = c;
        kb_input.dwFlags = KEYEVENTF_UNICODE;
        if up {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }

        let mut input: INPUT = mem::zeroed();
        input.type_ = INPUT_KEYBOARD;
        *input.u.ki_mut() = kb_input;

        SendInput(1, &mut input, mem::size_of_val(&input) as _);
    }
}

fn send_key(scan_code: u16, up: bool, vk: u8) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wVk = vk as _;
        kb_input.wScan = scan_code;
        if up {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }

        let mut input: INPUT = mem::zeroed();
        input.type_ = INPUT_KEYBOARD;
        *input.u.ki_mut() = kb_input;

        SendInput(1, &mut input, mem::size_of_val(&input) as _);
    }
}

fn send_char(scan_code: u16, up: bool, c: u16) {
    unsafe {
        // TODO: Improve layout handling
        let vk_state = VkKeyScanExW(c, GetKeyboardLayout(0));

        // Send the character as unicode input if:
        // 1. There is no key for the character available on the current keyboard layout
        // 2. A modifier (bits the upper byte) is required to type this character
        if vk_state == -1 || vk_state & 0xF00 != 0 {
            send_unicode(up, c);
        } else {
            send_key(scan_code, up, vk_state as u8);
        }
    }
}

fn main() {
    let rows = [
        (0x10, OsStr::new("bu.,üpclmfx´")),
        (0x1E, OsStr::new("hieaodtrnsß")),
        (0x2C, OsStr::new("kyöäqjgwvz")),
    ];

    let mut key_map = HashMap::new();
    for (scan_code, row_map) in &rows {
        for (i, key) in row_map.encode_wide().enumerate() {
            key_map.insert(scan_code + i as u16, key);
        }
    }

    KeyboardHook::set(move |scan_code, up| {
        if let Some(&c) = key_map.get(&scan_code) {
            println!("{}", c);
            send_char(scan_code, up, c);
            false
        } else {
            true
        }
    });

    unsafe {
        let mut msg: MSG = mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&mut msg);
            DispatchMessageW(&mut msg);
        }
    }
}
