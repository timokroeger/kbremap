#![windows_subsystem = "windows"]

mod keyboard_hook;

use std::{
    collections::HashMap,
    convert::TryInto,
    fs,
    sync::atomic::{AtomicBool, Ordering},
};

use toml::Value;
use trayicon::{Icon, MenuBuilder, TrayIconBuilder};
use winapi::um::consoleapi::*;
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
};

use keyboard_hook::{KeyboardHook, Remap};

static BYPASS: AtomicBool = AtomicBool::new(false);

fn main() {
    let mut base_layer = HashMap::new();
    let mut layers = Vec::new();
    if let Ok(config_str) = fs::read_to_string("config.toml") {
        // TODO: Improve error reporting (there is no console to print the panic).
        let config: Value = config_str.parse().unwrap();
        for (_layer_name, layer) in config["layers"].as_table().unwrap() {
            let mut map = HashMap::new();
            for mapping in layer["map"].as_array().unwrap() {
                let scan_code: u16 = mapping["scan_code"]
                    .as_integer()
                    .unwrap()
                    .try_into()
                    .unwrap();
                let characters = mapping["characters"].as_str().unwrap();
                for (i, key) in characters.chars().enumerate() {
                    map.insert(scan_code + i as u16, key);
                }
            }

            if let Some(mods) = layer.get("modifiers").and_then(Value::as_array) {
                layers.push((
                    mods.iter()
                        .map(|sc| sc.as_integer().unwrap() as u16)
                        .collect::<Vec<u16>>(),
                    map,
                ));
            } else {
                base_layer.extend(map);
            }
        }
    }

    let mut layer_modifiers = Vec::new();
    let _kbhook = KeyboardHook::set(|key| {
        if BYPASS.load(Ordering::SeqCst) {
            return Remap::Transparent;
        }

        // Check if we received an already active layer modifier key.
        if let Some(pos) = layer_modifiers
            .iter()
            .rposition(|&scan_code| key.scan_code() == scan_code)
        {
            if key.up() {
                layer_modifiers.remove(pos);
            }
            // Also ignore repeated down events.
            return Remap::Ignore;
        }

        // Check if we need to activate a layer.
        if key.down()
            && layers
                .iter()
                .any(|(modifiers, _)| modifiers.contains(&key.scan_code()))
        {
            // Activate layer by pushing the modifier onto a stack.
            layer_modifiers.push(key.scan_code());
            return Remap::Ignore;
        }

        // Select the layer the user activated most recently.
        let active_layer = if let Some(lmod) = layer_modifiers.last() {
            &layers
                .iter()
                .find(|(modifiers, _)| modifiers.contains(lmod))
                .unwrap()
                .1
        } else {
            &base_layer
        };

        let remapped_char = active_layer.get(&key.scan_code());
        match remapped_char {
            Some(&c) => Remap::Character(c),
            None => Remap::Transparent,
        }
    });

    // UI code.
    // The `trayicon` crate provides a nice declarative interface which plays
    // well with the `winit` as abstraction layer over the WinAPI message loop.
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum Events {
        ToggleEnabled,
        DebugOutput,
        Exit,
    };
    let event_loop = EventLoop::<Events>::with_user_event();
    let event_loop_proxy = event_loop.create_proxy();

    // Include the icons as resources.
    let icon_enabled = include_bytes!("../icons/keyboard.ico");
    let icon_disabled = include_bytes!("../icons/keyboard_delete.ico");

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

    // Construct the `Icon`s here, after creating the tray, because the builder
    // requires the raw icon resource which get's consumed now.
    let icon_enabled = Icon::from_buffer(icon_enabled, None, None).unwrap();
    let icon_disabled = Icon::from_buffer(icon_disabled, None, None).unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(Events::ToggleEnabled) => {
                // 1 xor 1 = 0
                // 0 xor 1 = 1
                if !BYPASS.fetch_xor(true, Ordering::SeqCst) {
                    tray_icon.set_icon(&icon_disabled).unwrap();
                } else {
                    tray_icon.set_icon(&icon_enabled).unwrap();
                }
            }
            Event::UserEvent(Events::DebugOutput) => unsafe {
                AllocConsole();
                // TODO: Do stop the process when closing the console.
            },
            Event::UserEvent(Events::Exit) => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}
