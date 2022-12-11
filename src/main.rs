#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{env, fs, io};

mod resources;
mod tray_icon;
mod winapi_util;

use anyhow::anyhow;
use kbremap::config::Config;
use kbremap::keyboard_hook::{self, KeyEvent, KeyType, KeyboardHook};
use kbremap::layout::KeyAction;
use kbremap::virtual_keyboard::VirtualKeyboard;
use single_instance::SingleInstance;
use winapi::um::winuser::*; // Virtual key constants VK_*

use crate::tray_icon::TrayIcon;

/// Custom keyboard layouts for windows.
#[derive(argh::FromArgs)]
struct CommandLineArguments {
    /// path to configuration file (default: `config.toml`)
    #[argh(option)]
    config: Option<String>,
}

fn config_path(config_file: &str) -> io::Result<PathBuf> {
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

    fs::canonicalize(config_file)
}

fn main() -> anyhow::Result<()> {
    // Display debug and panic output when launched from a terminal.
    let console_available = unsafe {
        use winapi::um::wincon::*;
        AttachConsole(ATTACH_PARENT_PROCESS) != 0
    };

    let args: CommandLineArguments = argh::from_env();
    let config_file = config_path(args.config.as_deref().unwrap_or("config.toml"))?;

    // Prevent duplicate instances if windows re-runs autostarts when rebooting after OS updates.
    let mut hasher = DefaultHasher::new();
    env::current_exe()?.hash(&mut hasher);
    config_file.hash(&mut hasher);
    let instance_key = format!("kbremap-{:016x}", hasher.finish());
    let instance = SingleInstance::new(&instance_key)?;
    if !instance.is_single() {
        return Err(anyhow!("already running with the same configuration"));
    }

    let config_str = fs::read_to_string(config_file)?;
    let config = Config::from_toml(&config_str)?;

    let layout = config.to_layout();

    native_windows_gui::init()?;
    let ui = Rc::new(TrayIcon::new(console_available)?);

    let mut kb = VirtualKeyboard::new(layout);
    let mut locked_layer = kb.locked_layer().to_string();
    let weak_ui = Rc::downgrade(&ui);
    let kbhook = KeyboardHook::set(move |mut key_event| {
        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling
        if let Some(caps_lock_layer) = &config.caps_lock_layer {
            if (kb.locked_layer() == caps_lock_layer) != keyboard_hook::caps_lock_enabled() {
                println!("toggle caps lock");
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

        if kb.locked_layer() != locked_layer {
            locked_layer = kb.locked_layer().to_string();
            weak_ui
                .upgrade()
                .unwrap()
                .show_message(&format!("Layer \"{locked_layer}\" locked",));
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

        println!("{}", log_line);
        handled
    });

    ui.set_hook(kbhook);

    // The event loop is also required for the low-level keyboard hook to work.
    native_windows_gui::dispatch_thread_events();

    Ok(())
}
