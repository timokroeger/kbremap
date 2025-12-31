#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod resources;
mod winapi;

use std::cell::Cell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

use anyhow::{Context, Result};
use kbremap::{Config, KeyAction, ReadableConfig, VirtualKeyboard};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;
use windows_sys::Win32::UI::WindowsAndMessaging::{MF_CHECKED, MF_DISABLED};

use crate::winapi::keyboard::{self, KeyEvent, KeyType};
use crate::winapi::{AutoStartEntry, StaticIcon, TrayIcon, TrayIconEvent};

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

fn load_config() -> Result<Config> {
    let config_file = env::args_os()
        .nth(1)
        .unwrap_or_else(|| "config.toml".into());
    let config_file = config_path(&config_file)?;
    let config_str = fs::read_to_string(config_file)?;
    let config: ReadableConfig = toml::from_str(&config_str)?;
    Ok(Config::try_from(config)?)
}

struct App {
    running_in_terminal: bool,
    autostart: AutoStartEntry<'static>,
    tray_icon: TrayIcon,
    enabled: Cell<bool>,
}

impl App {
    fn new() -> Self {
        keyboard::hook_enable();
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
            keyboard::hook_disable();
        } else {
            self.tray_icon
                .set_icon(StaticIcon::from_rc_numeric(resources::ICON_KEYBOARD));
            self.enabled.set(true);
            keyboard::hook_enable();
        }
    }
}

async fn remap_keys(config: Config) {
    let mut kb = VirtualKeyboard::new(config.layout);
    loop {
        let mut key_event = keyboard::next_key_event().await;

        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling:
        // Make sure the caps lock state stays in sync with the configured layer.
        if let Some(caps_lock_layer) = &config.caps_lock_layer {
            if (kb.locked_layer() == caps_lock_layer) != keyboard::caps_lock_enabled() {
                keyboard::send_key(KeyEvent {
                    up: false,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
                keyboard::send_key(KeyEvent {
                    up: true,
                    key: KeyType::VirtualKey(VK_CAPITAL as _),
                    ..key_event
                });
                println!("caps lock toggled");
            }
        }

        match remap {
            None => println!("{key_event} forwarded"),
            Some(KeyAction::Ignore) => {
                println!("{key_event} ignored");
                continue;
            }
            Some(KeyAction::Character(c)) => {
                if let Some(virtual_key) = keyboard::get_virtual_key(c) {
                    println!("{key_event} remapped to `{c}` as virtual key");
                    key_event.key = KeyType::VirtualKey(virtual_key);
                } else {
                    println!("{key_event} remapped to `{c}` as unicode input");
                    key_event.key = KeyType::Unicode(c);
                }
            }
            Some(KeyAction::VirtualKey(virtual_key)) => {
                println!("{key_event} remapped to virtual key {virtual_key:#04X}");
                key_event.key = KeyType::VirtualKey(virtual_key);
            }
        }

        keyboard::send_key(key_event);
    }
}

fn main() -> Result<()> {
    let app = Box::leak(Box::new(App::new()));

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

    // The executor runs the windows message loop internally.
    winmsg_executor::block_on(remap_keys(load_config()?));

    Ok(())
}
