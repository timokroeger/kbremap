#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod config;
mod keyboard_hook;
mod layers;
mod resources;
mod tray_icon;
mod winapi_util;

use std::fs;

use anyhow::Result;
use config::Config;
use keyboard_hook::KeyboardHook;
use layers::Layers;

use crate::tray_icon::TrayIcon;

/// Custom keyboard layouts for windows.
#[derive(argh::FromArgs)]
struct CommandLineArguments {
    /// path to configuration file (default: `config.toml`)
    #[argh(option)]
    config: Option<String>,
}

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

    native_windows_gui::init()?;
    let ui = TrayIcon::new()?;

    let mut layers = Layers::new(&config)?;

    let kbhook = KeyboardHook::set(|key| {
        if !ui.is_enabled() {
            return None;
        }

        layers.get_remapping(key.scan_code, key.up)
    });
    kbhook.disable_caps_lock(config.disable_caps_lock);

    // The event loop is also required for the low-level keyboard hook to work.
    native_windows_gui::dispatch_thread_events();

    Ok(())
}
