#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{env, fs};

mod resources;
mod tray_icon;
mod winapi_util;

use anyhow::{anyhow, Context, Result};
use kbremap::config::Config;
use kbremap::keyboard_hook::{self, KeyEvent, KeyType, KeyboardHook};
use kbremap::layout::KeyAction;
use kbremap::virtual_keyboard::VirtualKeyboard;
use single_instance::SingleInstance;
use winapi::um::winuser::*; // Virtual key constants VK_*

use crate::tray_icon::TrayIcon;

fn config_path(config_file: &OsStr) -> Result<PathBuf> {
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

    config_file.canonicalize().context(format!(
        "Cannot load configuration {}",
        config_file.display()
    ))
}

fn main() -> Result<()> {
    // Display debug and panic output when launched from a terminal.
    let mut console_available = false;
    unsafe {
        use winapi::um::wincon::*;
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            console_available = true;
            winapi_util::disable_quick_edit_mode();
        }
    };

    let config_file = env::args_os()
        .nth(1)
        .unwrap_or_else(|| "config.toml".into());
    let config_file = config_path(&config_file)?;

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
    let mut kb = VirtualKeyboard::new(layout);

    native_windows_gui::init()?;
    let ui = Rc::new(TrayIcon::new(console_available, &kb)?);

    // Use a weak pointer to prevent a cyclic reference between the UI and the
    // hook callback.
    let ui_hook = Rc::downgrade(&ui);

    let kbhook = KeyboardHook::set(move |mut key_event| {
        // The UI must not be invalidated before unregistering the hook.
        // We can unwrap here because we tranfer ownership of the hook to the UI.
        let ui = ui_hook.upgrade().unwrap();

        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling:
        // Make sure the caps lock state stays in sync with the configured layer.
        if let Some(caps_lock_layer) = config.caps_lock_layer() {
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

        ui.set_active_layer(kb.active_layer());
        ui.set_locked_layer(kb.locked_layer());

        let Some(remap) = remap else {
            return false;
        };

        match remap {
            KeyAction::Ignore => {
                println!("{} ignored", key_event);
            }
            KeyAction::Character(c) => {
                if let Some(virtual_key) = keyboard_hook::get_virtual_key(c) {
                    println!("{} remapped to `{}` as virtual key", key_event, c);
                    key_event.key = KeyType::VirtualKey(virtual_key);
                } else {
                    println!("{} remapped to `{}` as unicode input", key_event, c);
                    key_event.key = KeyType::Unicode(c);
                }
                keyboard_hook::send_key(key_event);
            }
            KeyAction::VirtualKey(virtual_key) => {
                println!("{} remapped to virtual key {:#04X}", key_event, virtual_key);
                key_event.key = KeyType::VirtualKey(virtual_key);
                keyboard_hook::send_key(key_event);
            }
        }
        true
    });

    ui.set_hook(kbhook);

    // The event loop is also required for the low-level keyboard hook to work.
    native_windows_gui::dispatch_thread_events();

    Ok(())
}
