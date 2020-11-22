mod keyboard_hook;

use std::{collections::HashMap, ffi::OsStr, mem, os::windows::ffi::OsStrExt};

use keyboard_hook::{KeyboardEvent, KeyboardHook};
use winapi::{shared::windef::*, um::winuser::*};

fn send_unicode(kb_event: &KeyboardEvent, c: u16) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wScan = c;
        kb_input.dwFlags = KEYEVENTF_UNICODE;
        if kb_event.up() {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }

        let mut input: INPUT = mem::zeroed();
        input.type_ = INPUT_KEYBOARD;
        *input.u.ki_mut() = kb_input;

        SendInput(1, &mut input, mem::size_of_val(&input) as _);
    }
}

fn send_key(kb_event: &KeyboardEvent, vk: u8) {
    unsafe {
        let mut kb_input: KEYBDINPUT = mem::zeroed();
        kb_input.wVk = vk as _;
        kb_input.wScan = kb_event.scan_code();
        if kb_event.up() {
            kb_input.dwFlags |= KEYEVENTF_KEYUP;
        }

        let mut input: INPUT = mem::zeroed();
        input.type_ = INPUT_KEYBOARD;
        *input.u.ki_mut() = kb_input;

        SendInput(1, &mut input, mem::size_of_val(&input) as _);
    }
}

fn send_char(kb_event: &KeyboardEvent, c: u16) {
    unsafe {
        // TODO: Improve layout handling
        let vk_state = VkKeyScanExW(c, GetKeyboardLayout(0));

        // Send the character as unicode input if:
        // 1. There is no key for the character available on the current keyboard layout
        // 2. A modifier (bits the upper byte) is required to type this character
        if vk_state == -1 || vk_state & 0xF00 != 0 {
            send_unicode(kb_event, c);
        } else {
            send_key(kb_event, vk_state as u8);
        }
    }
}

fn main() {
    let mut l1 = HashMap::new();
    for (scan_code, row_map) in &[
        (0x10, OsStr::new("bu.,üpclmfx´")),
        (0x1E, OsStr::new("hieaodtrnsß")),
        (0x2C, OsStr::new("kyöäqjgwvz")),
    ] {
        for (i, key) in row_map.encode_wide().enumerate() {
            l1.insert(scan_code + i as u16, key);
        }
    }

    let mut l3 = HashMap::new();
    for (scan_code, row_map) in &[
        (0x10, OsStr::new("…_[]^!<>=&")),
        (0x1E, OsStr::new("\\/{}*?()-:@")),
        (0x2C, OsStr::new("#$|~`+%\"';")),
    ] {
        for (i, key) in row_map.encode_wide().enumerate() {
            l3.insert(scan_code + i as u16, key);
        }
    }

    let mut l3_active = false;

    KeyboardHook::set(move |kb_event| {
        println!(
            "{}{}{} scan code: {:#06X}, virtual key: {:#04X}",
            if kb_event.up() { '↑' } else { '↓' },
            if kb_event.is_injected() { 'i' } else { ' ' },
            if kb_event.is_extended() { 'e' } else { ' ' },
            kb_event.scan_code(),
            kb_event.virtual_key(),
        );

        // Do not map out injected and extended scan codes.
        if kb_event.is_injected() || kb_event.is_extended() {
            return true;
        }

        // Layer3 is activated by the `caps lock` or `#` key.
        if kb_event.scan_code() == 0x3A || kb_event.scan_code() == 0x2B {
            l3_active = !kb_event.up();
            return false;
        }

        let remapped_char = if l3_active {
            l3.get(&kb_event.scan_code())
                .or_else(|| l1.get(&kb_event.scan_code()))
        } else {
            l1.get(&kb_event.scan_code())
        };

        match remapped_char {
            Some(&c) => {
                send_char(kb_event, c);
                false
            }
            None => true,
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
