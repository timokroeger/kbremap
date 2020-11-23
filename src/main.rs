#![windows_subsystem = "windows"]

mod keyboard_hook;

use std::{collections::HashMap, ffi::OsStr, mem, os::windows::ffi::OsStrExt};

use keyboard_hook::{KeyboardEvent, KeyboardHook};

use trayicon::{Icon, MenuBuilder, TrayIconBuilder};
use winapi::um::{consoleapi::*, winuser::*};
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
};

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
        kb_input.wVk = vk as u16;
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
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum Events {
        ToggleEnabled,
        DebugOutput,
        Exit,
    };
    let event_loop = EventLoop::<Events>::with_user_event();
    let event_loop_proxy = event_loop.create_proxy();

    let icon_enabled = include_bytes!("keyboard.ico");
    let icon_disabled = include_bytes!("keyboard_delete.ico");

    // Double click to exit.
    let mut tray_icon = TrayIconBuilder::new()
        .sender_winit(event_loop_proxy)
        .icon_from_buffer(icon_enabled)
        .on_click(Events::ToggleEnabled)
        .menu(
            MenuBuilder::new()
                .item("Show debug output", Events::DebugOutput)
                .item("E&xit", Events::Exit),
        )
        .build()
        .unwrap();
    let icon_enabled = Icon::from_buffer(icon_enabled, None, None).unwrap();
    let icon_disabled = Icon::from_buffer(icon_disabled, None, None).unwrap();

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

    let mut bypass = false;
    let mut l3_active = false;

    let _kbhook = KeyboardHook::set(move |kb_event| {
        if bypass {
            return true;
        }

        // TODO: Allow to remap extended scan codes.
        if kb_event.is_extended() {
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

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(Events::ToggleEnabled) => {
                bypass = !bypass;
                if bypass {
                    tray_icon.set_icon(&icon_disabled).unwrap();
                } else {
                    tray_icon.set_icon(&icon_enabled).unwrap();
                }
            }
            Event::UserEvent(Events::DebugOutput) => unsafe {
                AllocConsole();
            },
            Event::UserEvent(Events::Exit) => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}
