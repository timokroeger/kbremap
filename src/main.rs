#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod resources;
mod winapi;

use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{anyhow, Context, Result};
use kbremap::{Config, KeyAction, ReadableConfig, VirtualKeyboard};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winmsg_executor::{quit_message_loop, run_message_loop_with_dispatcher};

use crate::winapi::{AutoStartEntry, Icon, KeyEvent, KeyType, KeyboardHook, PopupMenu, TrayIcon};

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

fn register_keyboard_hook(
    config: &'static Config,
) -> KeyboardHook<impl FnMut(KeyEvent) -> bool + 'static> {
    let mut kb = VirtualKeyboard::new(&config.layout);

    KeyboardHook::set(move |mut key_event| {
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

        //ui.set_active_layer(kb.active_layer());
        //ui.set_locked_layer(kb.locked_layer());

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
    })
}

fn main() -> Result<()> {
    // Display debug and panic output when launched from a terminal.
    // Not only checks if we are running from a terminal but also attaches to it.
    let running_in_terminal = winapi::console_check();

    // Prevent duplicate instances if windows re-runs autostarts when rebooting after OS updates.
    let current_exe = env::current_exe()?;
    let instance_key = CString::new(current_exe.to_string_lossy().as_bytes())?;
    if !winapi::register_instance(&instance_key) {
        return Err(anyhow!("already running with the same configuration"));
    }

    let config = load_config()?;
    let config = Box::leak(Box::new(config));

    let kbhook = &RefCell::new(Some(register_keyboard_hook(config)));

    // UI code

    // Load resources
    let icon_enabled = Icon::from_rc_numeric(resources::ICON_KEYBOARD);
    let icon_disabled = Icon::from_rc_numeric(resources::ICON_KEYBOARD_DELETE);

    // Arbitrary ID in the WM_APP range, used to identify which tray icon a message originates from.
    const WM_APP_TRAYICON: u32 = WM_APP + 873;
    let tray_icon = TrayIcon::new(WM_APP_TRAYICON, icon_enabled.0);

    let autostart = AutoStartEntry::new(c"kbremap");

    // Enabled state can be changed by double click to the tray icon or from the context menu.
    let toggle_enabled = || {
        let mut kbhook = kbhook.borrow_mut();
        if kbhook.is_some() {
            *kbhook = None;
            tray_icon.set_icon(icon_disabled.0);
        } else {
            *kbhook = Some(register_keyboard_hook(config));
            tray_icon.set_icon(icon_enabled.0);
        }
    };

    // Event loop required for the low-level keyboard hook and the tray icon.
    run_message_loop_with_dispatcher(move |msg| {
        if msg.message != WM_APP_TRAYICON {
            return false;
        }

        let event = msg.lParam as u32;
        if event == WM_LBUTTONDBLCLK {
            toggle_enabled();
            true
        } else if event == WM_RBUTTONUP {
            const MENU_STARTUP: u32 = 1;
            const MENU_DEBUG: u32 = 2;
            const MENU_DISABLE: u32 = 3;
            const MENU_EXIT: u32 = 4;

            let flag_checked = |condition| if condition { MF_CHECKED } else { 0 };
            let flag_disabled = |condition| if condition { MF_DISABLED } else { 0 };

            let menu = PopupMenu::new();
            menu.add_entry(
                MENU_STARTUP,
                flag_checked(autostart.is_registered()),
                c"Run at system startup",
            );
            menu.add_entry(
                MENU_DEBUG,
                flag_checked(winapi::console_check()) | flag_disabled(running_in_terminal),
                c"Show debug output",
            );
            menu.add_entry(
                MENU_DISABLE,
                flag_checked(kbhook.borrow().is_none()),
                c"Disable",
            );
            menu.add_entry(MENU_EXIT, 0, c"Exit");

            match menu.show(msg.hwnd, msg.pt) {
                Some(MENU_STARTUP) => {
                    if autostart.is_registered() {
                        autostart.remove();
                    } else {
                        autostart.register();
                    }
                }
                Some(MENU_DEBUG) => {
                    // Toggle console window to display debug logs.
                    if winapi::console_check() {
                        winapi::console_close()
                    } else {
                        winapi::console_open()
                    }
                }
                Some(MENU_DISABLE) => toggle_enabled(),
                Some(MENU_EXIT) => quit_message_loop(),
                Some(_) => unreachable!(),
                _ => {}
            }
            true
        } else {
            false
        }
    });

    Ok(())
}
