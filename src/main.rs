#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::{env, fs, ptr};

mod resources;
mod tray_icon;
mod winapi_util;

use anyhow::{anyhow, Context, Result};
use kbremap::config::Config;
use kbremap::keyboard_hook::{self, KeyEvent, KeyType, KeyboardHook};
use kbremap::layout::KeyAction;
use kbremap::virtual_keyboard::VirtualKeyboard;
use widestring::{u16cstr, U16CString};
use winapi_util::register_instance;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::tray_icon::TrayIcon;
use crate::winapi_util::AutoStartEntry;

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
    let instance_key = U16CString::from_str(format!("kbremap-{:016x}", hasher.finish())).unwrap();
    if !register_instance(instance_key.as_ucstr()) {
        return Err(anyhow!("already running with the same configuration"));
    }

    let config_str = fs::read_to_string(config_file)?;
    let config = Config::from_toml(&config_str)?;

    let layout = config.to_layout();
    let mut kb = VirtualKeyboard::new(layout);

    let mut kbhook = KeyboardHook::set(move |mut key_event| {
        let remap = if key_event.up {
            kb.release_key(key_event.scan_code)
        } else {
            kb.press_key(key_event.scan_code)
        };

        // Special caps lock handling:
        // Make sure the caps lock state stays in sync with the configured layer.
        if let Some(caps_lock_layer) = config.caps_lock_layer() {
            if (kb.locked_layer() == caps_lock_layer) != keyboard_hook::caps_lock_enabled() {
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

    // UI code
    // The event loop is also required for the low-level keyboard hook to work.

    // Load resources
    let icon_enabled = winapi_util::icon_from_rc_numeric(resources::ICON_KEYBOARD);
    let icon_disabled = winapi_util::icon_from_rc_numeric(resources::ICON_KEYBOARD_DELETE);
    let menu = winapi_util::popupmenu_from_rc_numeric(resources::MENU);

    const WM_APP_TRAYICON: u32 = WM_APP + 873;
    let tray_icon = TrayIcon::new(WM_APP_TRAYICON, icon_enabled);

    let cmd = env::current_exe().unwrap();
    let autostart = AutoStartEntry::new(
        u16cstr!("kbremap").into(),
        U16CString::from_os_str(cmd).unwrap(),
    );

    // Disable the debug output entry if the tool runs from command line.
    if console_available {
        unsafe { EnableMenuItem(menu, resources::MENU_DEBUG.into(), MF_DISABLED) };
    }

    let toggle_enabled = |kbhook: &mut KeyboardHook| {
        if kbhook.active() {
            kbhook.disable();
            tray_icon.set_icon(icon_disabled);
        } else {
            kbhook.enable();
            tray_icon.set_icon(icon_enabled);
        }
    };

    // Event loop required for the low-level keyboard hook and the tray icon.
    winapi_util::message_loop(|msg| match (msg.message, msg.lParam as _) {
        (WM_APP_TRAYICON, WM_LBUTTONDBLCLK) => {
            toggle_enabled(&mut kbhook);
        }
        (WM_APP_TRAYICON, WM_RBUTTONUP) => unsafe {
            // Refresh menu state
            CheckMenuItem(
                menu,
                resources::MENU_DISABLE.into(),
                if kbhook.active() {
                    MF_UNCHECKED
                } else {
                    MF_CHECKED
                },
            );

            AttachConsole(ATTACH_PARENT_PROCESS);
            CheckMenuItem(
                menu,
                resources::MENU_DEBUG.into(),
                if GetLastError() == ERROR_INVALID_HANDLE {
                    MF_UNCHECKED
                } else {
                    MF_CHECKED
                },
            );

            CheckMenuItem(
                menu,
                resources::MENU_STARTUP.into(),
                if autostart.is_registered() {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                },
            );

            // Required for the menu to disappear when it loses focus.
            SetForegroundWindow(msg.hwnd);
            let menu_selection = TrackPopupMenuEx(
                menu,
                TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
                msg.pt.x,
                msg.pt.y,
                msg.hwnd,
                ptr::null(),
            );
            match menu_selection as u16 {
                resources::MENU_EXIT => PostQuitMessage(0),
                resources::MENU_DISABLE => toggle_enabled(&mut kbhook),
                resources::MENU_DEBUG => {
                    // Toggle console window to display debug logs.
                    AttachConsole(ATTACH_PARENT_PROCESS);
                    if GetLastError() == ERROR_INVALID_HANDLE {
                        AllocConsole();
                        winapi_util::disable_quick_edit_mode();
                        let console = GetConsoleWindow();
                        let console_menu = GetSystemMenu(console, 0);
                        DeleteMenu(console_menu, SC_CLOSE as _, MF_BYCOMMAND);
                    } else {
                        FreeConsole();
                    }
                }
                resources::MENU_STARTUP => {
                    if autostart.is_registered() {
                        autostart.remove();
                    } else {
                        autostart.register();
                    }
                }
                _ => {}
            }
        },
        _ => {}
    });

    Ok(())
}
