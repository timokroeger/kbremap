#![windows_subsystem = "windows"]

mod config;
mod keyboard_hook;

use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};

use config::Config;
use keyboard_hook::{KeyboardHook, Remap};
use trayicon::{Icon, MenuBuilder, TrayIconBuilder};
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
};

static BYPASS: AtomicBool = AtomicBool::new(false);

fn main() -> anyhow::Result<()> {
    // Display debug and panic output when launched from a terminal.
    unsafe {
        use winapi::um::wincon::*;
        AttachConsole(ATTACH_PARENT_PROCESS);
    };

    let config_str = fs::read_to_string("config.toml")?;
    let config = Config::from_toml(&config_str)?;

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
        if key.down() && config.is_layer_modifier(key.scan_code()) {
            // Activate layer by pushing the modifier onto a stack.
            layer_modifiers.push(key.scan_code());
            return Remap::Ignore;
        }

        // Select the layer the user activated most recently.
        let active_layer = if let Some(&lmod) = layer_modifiers.last() {
            config.layer_map(lmod).unwrap()
        } else {
            &config.base_layer_map()
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
        .menu(MenuBuilder::new().item("E&xit", Events::Exit))
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
            Event::UserEvent(Events::Exit) => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}
