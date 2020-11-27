#![windows_subsystem = "windows"]

mod config;
mod keyboard_hook;
mod layers;

use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use config::Config;
use keyboard_hook::{KeyboardHook, Remap};
use trayicon::{Icon, MenuBuilder, TrayIconBuilder};
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop},
};

static BYPASS: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    // Display debug and panic output when launched from a terminal.
    unsafe {
        use winapi::um::wincon::*;
        AttachConsole(ATTACH_PARENT_PROCESS);
    };

    let config_str = fs::read_to_string("config.toml")?;
    let mut layers = Config::from_toml(&config_str)?;
    layers.build_activation_sequences("base");

    let _kbhook = KeyboardHook::set(|key| {
        if BYPASS.load(Ordering::SeqCst) {
            return Remap::Transparent;
        }

        layers.process_modifiers(key);
        layers.get_remapping(key.scan_code())
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
