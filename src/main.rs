#![cfg_attr(not(test), windows_subsystem = "windows")]
#![cfg_attr(test, windows_subsystem = "console")]

mod config;
mod keyboard_hook;
mod layers;
mod resources;
mod tray_icon;
mod win32_wrappers;

use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, mem, ptr};

use anyhow::Result;
use config::Config;
use keyboard_hook::{KeyboardHook, Remap};
use layers::Layers;
use tray_icon::{Event, TrayIcon};
use wchar::wchz;
use win32_wrappers::MessageOnlyWindow;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::*;
use winapi::um::winuser::*;

const WM_APP_KBREMAP: u32 = WM_APP + 738;

/// Custom keyboard layouts for windows.
#[derive(argh::FromArgs)]
struct CommandLineArguments {
    /// path to configuration file (default: `config.toml`)
    #[argh(option)]
    config: Option<String>,
}

/// No keys are remapped when set to `true`.
static BYPASS: AtomicBool = AtomicBool::new(false);

pub fn icon_from_rc_numeric(id: u16) -> HICON {
    let hicon =
        unsafe { LoadImageW(GetModuleHandleW(ptr::null()), id as _, IMAGE_ICON, 0, 0, 0) as _ };
    assert_ne!(hicon, ptr::null_mut(), "icon resource {} not found", id);
    hicon
}

pub fn popupmenu_from_rc_numeric(id: u16) -> HMENU {
    unsafe {
        let menu = LoadMenuA(GetModuleHandleA(ptr::null()), id as _);
        assert_ne!(menu, ptr::null_mut(), "menu resource {} not found", id);
        let submenu = GetSubMenu(menu, 0);
        assert_ne!(
            submenu,
            ptr::null_mut(),
            "menu resource {} requires a popup submenu item",
            id
        );
        submenu
    }
}

fn create_dummy_window() -> MessageOnlyWindow {
    unsafe {
        let mut wnd_class: WNDCLASSW = mem::zeroed();
        wnd_class.lpfnWndProc = Some(DefWindowProcW);
        wnd_class.lpszClassName = wchz!("menu").as_ptr();
        let wnd_class_atom = RegisterClassW(&wnd_class);
        assert_ne!(wnd_class_atom, 0);

        MessageOnlyWindow::new(wnd_class_atom as _)
    }
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

    let mut layers = Layers::new(&config)?;

    let _kbhook = KeyboardHook::set(move |key| {
        if BYPASS.load(Ordering::SeqCst) {
            return Remap::Transparent;
        }

        layers.get_remapping(key.scan_code(), key.up())
    });

    // UI code

    // Load resources
    let icon_active = icon_from_rc_numeric(resources::ICON_KEYBOARD);
    let icon_bypass = icon_from_rc_numeric(resources::ICON_KEYBOARD_DELETE);
    let menu = popupmenu_from_rc_numeric(resources::MENU);

    let tray_icon = TrayIcon::new(WM_APP_KBREMAP);
    tray_icon.set_icon(icon_active);

    // A dummy window handle is required to show a menu.
    let dummy_window = create_dummy_window();

    // Event loop required for the low-level keyboard hook and the tray icon.
    win32_wrappers::message_loop(move |msg| {
        if let Some(event_message) = tray_icon.event_from_message(&msg) {
            match event_message.event {
                Event::DoubleClick => {
                    // 1 xor 1 = 0
                    // 0 xor 1 = 1
                    if !BYPASS.fetch_xor(true, Ordering::SeqCst) {
                        tray_icon.set_icon(icon_bypass);
                    } else {
                        tray_icon.set_icon(icon_active);
                    }
                }
                Event::RightClick => unsafe {
                    TrackPopupMenuEx(
                        menu,
                        TPM_BOTTOMALIGN | TPM_NONOTIFY,
                        event_message.x.into(),
                        event_message.y.into(),
                        dummy_window.handle(),
                        ptr::null_mut(),
                    );
                },
            }
        } else if msg.message == WM_COMMAND && msg.wParam == resources::MENU_EXIT.into() {
            unsafe { PostQuitMessage(0) };
        }
    });

    Ok(())
}
