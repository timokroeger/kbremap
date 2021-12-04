#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

use std::path::Path;
use std::{env, fs};

mod resources;
mod tray_icon;
mod winapi_util;

use anyhow::Result;
use kbremap::config::Config;
use kbremap::keyboard_hook::{self, KeyEvent, KeyType, KeyboardHook};
use kbremap::layout::KeyAction;
use kbremap::virtual_keyboard::VirtualKeyboard;
use tracing::Level;
use winapi::um::winuser::*; // Virtual key constants VK_*

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
    let layout = config.to_layout();
    let caps_lock_layer_idx = config.caps_lock_layer.map(|l| {
        layout
            .layer_names()
            .iter()
            .position(|name| l == *name)
            .expect("caps lock layer not found") as u8
    });

    let mut kb = VirtualKeyboard::new(layout)?;

    let _kbhook = KeyboardHook::set(|mut key_event| {
        if !ui.is_enabled() {
            return false;
        }

        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling
        if let Some(caps_lock_layer) = caps_lock_layer_idx {
            if (kb.locked_layer() == caps_lock_layer) != keyboard_hook::caps_lock_enabled() {
                tracing::debug!("toggle caps lock");
                keyboard_hook::send_key(KeyEvent {
                    up: false,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
                keyboard_hook::send_key(KeyEvent {
                    up: true,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
            }
        }

        let mut log_line = key_event.to_string();

        let handled = if let Some(remap) = remap {
            match remap {
                KeyAction::Ignore => {
                    log_line.push_str(" ignored");
                }
                KeyAction::Character(c) => {
                    if let Some(virtual_key) = keyboard_hook::get_virtual_key(c) {
                        log_line = format!("{} remapped to `{}` as virtual key", log_line, c);
                        key_event.key = KeyType::VirtualKey(virtual_key);
                    } else {
                        log_line = format!("{} remapped to `{}` as unicode input", log_line, c);
                        key_event.key = KeyType::Unicode(c);
                    }
                    keyboard_hook::send_key(key_event);
                }
                KeyAction::VirtualKey(virtual_key) => {
                    log_line = format!("{} remapped to virtual key {:#04X}", log_line, virtual_key);
                    key_event.key = KeyType::VirtualKey(virtual_key);
                    keyboard_hook::send_key(key_event);
                }
            }
            true
        } else {
            false
        };

        tracing::debug!("{}", log_line);
        handled
    });

    // The event loop is also required for the low-level keyboard hook to work.
    native_windows_gui::dispatch_thread_events();

    Ok(())
}
