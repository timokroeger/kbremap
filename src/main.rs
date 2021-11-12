#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod config;
mod keyboard_hook;
mod layers;
mod tray_icon;

use std::{
    fs, mem, process, ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use config::Config;
use keyboard_hook::{KeyboardHook, Remap};
use layers::Layers;
use tray_icon::{IconResource, TrayIcon};
use winapi::um::winuser;

/// Custom keyboard layouts for windows. Fully configurable for quick prototyping of new layouts.
// As defined in `build.rs`
const RESOURCE_ID_ICON_KEYBOARD: u16 = 1;
const RESOURCE_ID_ICON_KEYBOARD_DELETE: u16 = 2;

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

    // UI code.
    let mut tray_icon = TrayIcon::new();
    tray_icon.set_icon(IconResource::load_numeric_id(RESOURCE_ID_ICON_KEYBOARD));

    // Event loop required for the low-level keyboard hook and the tray icon.
    unsafe {
        let mut msg = mem::zeroed();
        loop {
            match winuser::GetMessageA(&mut msg, ptr::null_mut(), 0, 0) {
                1 => {
                    // We only handle keyboard input in the low-level hook for now.
                    // winuser::TranslateMessage(&msg);

                    winuser::DispatchMessageA(&msg);
                }
                0 => process::exit(msg.wParam as _),
                _ => unreachable!(),
            }
        }
    }
}
