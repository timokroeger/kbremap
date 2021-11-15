#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod config;
mod keyboard_hook;
mod layers;
mod resources;
mod tray_icon;
mod win32_wrappers;

use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, ptr};

use anyhow::Result;
use config::Config;
use keyboard_hook::{KeyboardHook, Remap};
use layers::Layers;
use tray_icon::TrayIcon;
use winapi::um::winuser::*;

const WM_APP_TRAYICON: u32 = winapi::um::winuser::WM_APP + 873;

/// Custom keyboard layouts for windows.
#[derive(argh::FromArgs)]
struct CommandLineArguments {
    /// path to configuration file (default: `config.toml`)
    #[argh(option)]
    config: Option<String>,
}

/// No keys are remapped when set to `true`.
static BYPASS: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    // Display debug and panic output when launched from a terminal.
    unsafe {
        use winapi::um::wincon::*;
        AttachConsole(ATTACH_PARENT_PROCESS);
    };

    let args: CommandLineArguments = argh::from_env();

    let config_str = fs::read_to_string(args.config.as_deref().unwrap_or("config.toml"))?;
    let config = Config::from_toml(&config_str)?;

    // Spawn a console window if debug output was requested in the config and
    // if the exetable was not launched from a terminal.
    if config.debug_output {
        unsafe { winapi::um::consoleapi::AllocConsole() };
    }

    let mut layers = Layers::new(&config)?;

    let _kbhook = KeyboardHook::set(move |key| {
        if BYPASS.load(Ordering::SeqCst) {
            return Remap::Transparent;
        }

        layers.get_remapping(key.scan_code(), key.up())
    });

    // UI code

    // Load resources
    let icon_active = win32_wrappers::icon_from_rc_numeric(resources::ICON_KEYBOARD);
    let icon_bypass = win32_wrappers::icon_from_rc_numeric(resources::ICON_KEYBOARD_DELETE);
    let menu = win32_wrappers::popupmenu_from_rc_numeric(resources::MENU);

    let tray_icon = TrayIcon::new(WM_APP_TRAYICON);
    tray_icon.set_icon(icon_active);

    // A dummy window handle is required to show a menu.
    let dummy_window = win32_wrappers::create_dummy_window();

    // Event loop required for the low-level keyboard hook and the tray icon.
    win32_wrappers::message_loop(move |msg| {
        match (msg.message, msg.lParam as _) {
            (WM_APP_TRAYICON, WM_LBUTTONDBLCLK) => {
                // 1 xor 1 = 0
                // 0 xor 1 = 1
                if !BYPASS.fetch_xor(true, Ordering::SeqCst) {
                    tray_icon.set_icon(icon_bypass);
                } else {
                    tray_icon.set_icon(icon_active);
                }
            }
            (WM_APP_TRAYICON, WM_RBUTTONUP) => unsafe {
                SetForegroundWindow(dummy_window.handle());
                let menu_selection = TrackPopupMenuEx(
                    menu,
                    TPM_BOTTOMALIGN | TPM_NONOTIFY | TPM_RETURNCMD,
                    msg.pt.x,
                    msg.pt.y,
                    dummy_window.handle(),
                    ptr::null_mut(),
                );
                if menu_selection == resources::MENU_EXIT.into() {
                    PostQuitMessage(0);
                }
            },
            _ => (),
        }
    });

    Ok(())
}
