#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod config;
mod keyboard_hook;
mod layout;
mod resources;
mod tray_icon;
mod virtual_keyboard;
mod winapi_util;

use std::collections::HashMap;
use std::path::Path;
use std::{env, fs};

use anyhow::Result;
use config::Config;
use keyboard_hook::KeyboardHook;
use layout::LayoutBuilder;
use tracing::Level;
use virtual_keyboard::VirtualKeyboard;

use crate::keyboard_hook::KeyAction;
use crate::tray_icon::TrayIcon;

/// Custom keyboard layouts for windows.
#[derive(argh::FromArgs)]
struct CommandLineArguments {
    /// path to configuration file (default: `config.toml`)
    #[argh(option)]
    config: Option<String>,
}

fn load_config(config_file: &str) -> Result<Config> {
    let mut path_buf;
    let mut config_file = Path::new(config_file);

    // Could not find the configuration file in current working directory.
    // Check if a config file with same name exists next to our executable.
    if !config_file.exists() && config_file.is_relative() {
        path_buf = env::current_exe()?;
        path_buf.pop();
        path_buf.push(config_file);
        config_file = path_buf.as_path();
    }

    let config_str = fs::read_to_string(config_file)?;
    Ok(Config::from_toml(&config_str)?)
}

fn main() -> Result<()> {
    // Display debug and panic output when launched from a terminal.
    let console_available = unsafe {
        use winapi::um::wincon::*;
        AttachConsole(ATTACH_PARENT_PROCESS) != 0
    };

    let (stdout_nb, _guard) = tracing_appender::non_blocking(std::io::stdout());
    tracing_subscriber::fmt()
        .with_writer(stdout_nb)
        .with_max_level(Level::DEBUG)
        .without_time()
        .with_level(false)
        .with_target(false)
        .init();

    native_windows_gui::init()?;
    let ui = TrayIcon::new(console_available)?;

    let args: CommandLineArguments = argh::from_env();

    let config = load_config(args.config.as_deref().unwrap_or("config.toml"))?;

    let layout = {
        let mut layout_builder = LayoutBuilder::new();
        for layer in config.layer_names() {
            let modifiers: HashMap<u16, &str> = config.layer_modifiers(layer).collect();

            for (scan_code, action) in config.layer_mappings(layer) {
                if let Some(target_layer) = modifiers.get(&scan_code) {
                    let vk = match action {
                        KeyAction::Ignore => None,
                        KeyAction::VirtualKey(vk) => Some(vk),
                        _ => panic!("invalid modifer target"),
                    };
                    layout_builder.add_modifier(scan_code, layer, target_layer, vk);
                } else {
                    layout_builder.add_key(scan_code, layer, action);
                }
            }
        }
        layout_builder.build()
    };

    let mut kb = VirtualKeyboard::new(layout)?;

    let kbhook = KeyboardHook::set(|key| {
        if !ui.is_enabled() {
            return None;
        }

        if key.up {
            kb.release_key(key.scan_code)
        } else {
            kb.press_key(key.scan_code)
        }
    });
    kbhook.disable_caps_lock(config.disable_caps_lock);

    // The event loop is also required for the low-level keyboard hook to work.
    native_windows_gui::dispatch_thread_events();

    Ok(())
}
