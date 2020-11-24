#![windows_subsystem = "windows"]

mod keyboard_hook;

use std::collections::HashMap;

use keyboard_hook::{KeyboardHook, Remap};

use trayicon::{Icon, MenuBuilder, TrayIconBuilder};
use winapi::um::consoleapi::*;
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
};

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

    let mut base_layer = HashMap::new();
    for (scan_code, row_map) in &[
        (0x10, "bu.,üpclmfx´"),
        (0x1E, "hieaodtrnsß"),
        (0x2C, "kyöäqjgwvz"),
    ] {
        for (i, key) in row_map.chars().enumerate() {
            base_layer.insert(scan_code + i as u16, key);
        }
    }

    let mut symbol_layer = HashMap::new();
    for (scan_code, row_map) in &[
        (0x10, "…_[]^!<>=&"),
        (0x1E, "\\/{}*?()-:@"),
        (0x2C, "#$|~`+%\"';"),
    ] {
        for (i, key) in row_map.chars().enumerate() {
            symbol_layer.insert(scan_code + i as u16, key);
        }
    }

    let mut bypass = false;

    let layers = vec![
        (&[0x3A, 0x2B], symbol_layer), // Layer3 is activated by the `caps lock` or `#` key.
    ];
    let mut active_layers = Vec::new();

    let _kbhook = KeyboardHook::set(|kb_event| {
        if bypass {
            return Remap::Transparent;
        }

        // TODO: Allow to remap extended scan codes.
        if kb_event.is_extended() {
            return Remap::Transparent;
        }

        // Handle layer activation
        if let Some((_, layer)) = layers
            .iter()
            .find(|(&modifiers, _)| modifiers.contains(&kb_event.scan_code()))
        {
            if kb_event.down() {
                active_layers.push(layer);
            } else {
                // Remove from active layers
                active_layers
                    .iter()
                    .rposition(|&l| l == layer)
                    .map(|pos| active_layers.remove(pos));
            }
            return Remap::Ignore;
        }

        let remapped_char = active_layers
            .last()
            .map(|&x| x)
            .unwrap_or(&base_layer)
            .get(&kb_event.scan_code());

        match remapped_char {
            Some(&c) => Remap::Character(c),
            None => Remap::Transparent,
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
