#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod resources;
mod winapi;

use std::cell::Cell;
use std::future::pending;
use std::{env, process};

use anyhow::Result;
use kbremap::{Config, KeyAction, VirtualKeyboard};
use winapi::{AutoStartEntry, KeyEvent, KeyType, StaticIcon, TrayIcon, TrayIconEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[cfg(feature = "runtime-config")]
fn load_config() -> Result<Config> {
    use anyhow::Context;
    use std::{fs, path::PathBuf};

    let config_file = env::args_os()
        .nth(1)
        .unwrap_or_else(|| "config.toml".into());

    let mut config_file = PathBuf::from(config_file);

    // Could not find the configuration file in current working directory.
    // Check if a config file with same name exists next to our executable.
    if !config_file.exists() && config_file.is_relative() {
        let mut new_config = env::current_exe()?;
        new_config.pop();
        new_config.push(config_file);
        config_file = new_config;
    }

    config_file.canonicalize().context(format!(
        "Cannot load configuration {}",
        config_file.display()
    ))?;

    let config_str = fs::read_to_string(config_file)?;
    let config: kbremap::ReadableConfig = toml::from_str(&config_str)?;
    Ok(Config::try_from(config)?)
}

#[cfg(not(feature = "runtime-config"))]
fn load_config() -> Result<Config> {
    let config_bin = include_bytes!(concat!(env!("OUT_DIR"), "/config.bin"));
    Ok(postcard::from_bytes(config_bin)?)
}

struct App {
    running_in_terminal: bool,
    autostart: AutoStartEntry<'static>,
    tray_icon: TrayIcon,
    enabled: Cell<bool>,
}

impl App {
    fn new() -> Self {
        Self {
            // Display debug and panic output when launched from a terminal.
            // Not only checks if we are running from a terminal but also attaches to it.
            running_in_terminal: winapi::console_check(),
            autostart: AutoStartEntry::new(c"kbremap"),
            tray_icon: TrayIcon::new(StaticIcon::from_rc_numeric(resources::ICON_KEYBOARD)),
            enabled: Cell::new(true),
        }
    }

    fn toggle_autostart(&self) {
        if self.autostart.is_registered() {
            self.autostart.remove();
        } else {
            self.autostart.register();
        }
    }

    fn toggle_debug_console(&self) {
        if winapi::console_check() {
            winapi::console_close();
        } else {
            winapi::console_open();
        }
    }

    fn toggle_enabled(&self) {
        if self.enabled.get() {
            self.tray_icon
                .set_icon(StaticIcon::from_rc_numeric(resources::ICON_KEYBOARD_DELETE));
            self.enabled.set(false);
        } else {
            self.tray_icon
                .set_icon(StaticIcon::from_rc_numeric(resources::ICON_KEYBOARD));
            self.enabled.set(true);
        }
    }
}

fn main() -> Result<()> {
    let app: &App = Box::leak(Box::new(App::new()));

    let config = load_config()?;
    let mut kb = VirtualKeyboard::new(config.layout);
    winapi::register_keyboard_hook(move |mut key_event| {
        if !app.enabled.get() {
            kb.reset();
            println!("{} forwarded because remapping is disabled", key_event);
            return false;
        }

        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling:
        // Make sure the caps lock state stays in sync with the configured layer.
        if let Some(caps_lock_layer) = &config.caps_lock_layer {
            if (kb.locked_layer() == caps_lock_layer) != winapi::caps_lock_enabled() {
                winapi::send_key(KeyEvent {
                    up: false,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
                winapi::send_key(KeyEvent {
                    up: true,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
                println!("caps lock toggled");
            }
        }

        let Some(remap) = remap else {
            println!("{} forwarded", key_event);
            return false;
        };

        match remap {
            KeyAction::Ignore => {
                println!("{} ignored", key_event);
            }
            KeyAction::Character(c) => {
                if let Some(virtual_key) = winapi::get_virtual_key(c) {
                    println!("{} remapped to `{}` as virtual key", key_event, c);
                    key_event.key = KeyType::VirtualKey(virtual_key);
                } else {
                    println!("{} remapped to `{}` as unicode input", key_event, c);
                    key_event.key = KeyType::Unicode(c);
                }
                winapi::send_key(key_event);
            }
            KeyAction::VirtualKey(virtual_key) => {
                println!("{} remapped to virtual key {:#04X}", key_event, virtual_key);
                key_event.key = KeyType::VirtualKey(virtual_key);
                winapi::send_key(key_event);
            }
        }
        true
    });

    const MENU_STARTUP: u32 = 1;
    const MENU_DEBUG: u32 = 2;
    const MENU_DISABLE: u32 = 3;
    const MENU_EXIT: u32 = 4;

    app.tray_icon.on_menu(|menu| {
        let flag_checked = |condition| if condition { MF_CHECKED } else { 0 };
        let flag_disabled = |condition| if condition { MF_DISABLED } else { 0 };

        menu.add_entry(
            MENU_STARTUP,
            flag_checked(app.autostart.is_registered()),
            c"Run at system startup",
        );
        menu.add_entry(
            MENU_DEBUG,
            flag_checked(winapi::console_check()) | flag_disabled(app.running_in_terminal),
            c"Show debug output",
        );
        menu.add_entry(MENU_DISABLE, flag_checked(!app.enabled.get()), c"Disable");
        menu.add_entry(MENU_EXIT, 0, c"Exit");
    });

    app.tray_icon.on_event(|event| {
        match event {
            TrayIconEvent::Click => {} // ignore
            TrayIconEvent::DoubleClick => app.toggle_enabled(),
            TrayIconEvent::MenuItem(MENU_STARTUP) => app.toggle_autostart(),
            TrayIconEvent::MenuItem(MENU_DEBUG) => app.toggle_debug_console(),
            TrayIconEvent::MenuItem(MENU_DISABLE) => app.toggle_enabled(),
            TrayIconEvent::MenuItem(MENU_EXIT) => process::exit(0),
            TrayIconEvent::MenuItem(_) => unreachable!(),
        }
    });

    // Event loop required for the low-level keyboard hook and the tray icon.
    winmsg_executor::block_on(pending::<()>());

    Ok(())
}
