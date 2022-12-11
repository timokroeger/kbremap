use std::cell::RefCell;
use std::env;
use std::rc::Rc;

use kbremap::keyboard_hook::KeyboardHook;
use kbremap::virtual_keyboard::VirtualKeyboard;
use native_windows_gui::{
    EmbedResource, Event, EventHandler, GlobalCursor, Icon, Menu, MenuItem, MessageWindow,
    NwgError, RawEventHandler, TrayNotification,
};
use widestring::{u16cstr, U16CString};
use winapi::um::consoleapi::*;
use winapi::um::wincon::*;
use winapi::um::winuser::*;

use crate::resources;
use crate::winapi_util::AutoStartEntry;

struct State {
    hook: Option<KeyboardHook>,
    autostart: AutoStartEntry,
    locked_layer: String,
}

impl State {
    fn new(locked_layer: String) -> Self {
        let cmd = env::current_exe().unwrap();
        Self {
            hook: None,
            autostart: AutoStartEntry::new(
                u16cstr!("kbremap").into(),
                U16CString::from_os_str(cmd).unwrap(),
            ),
            locked_layer,
        }
    }
}

#[derive(Default)]
struct Handles {
    resources: EmbedResource,
    icon_enabled: Icon,
    icon_disabled: Icon,
    window: MessageWindow,
    tray: TrayNotification,
    tray_menu: Menu,
    tray_menu_autostart: MenuItem,
    tray_menu_debug: MenuItem,
    tray_menu_disable: MenuItem,
    tray_menu_exit: MenuItem,
}

struct TrayIconData {
    handles: Handles,
    state: RefCell<State>,
}

impl TrayIconData {
    fn new(state: State) -> Self {
        Self {
            handles: Default::default(),
            state: RefCell::new(state),
        }
    }

    fn show_menu(&self) {
        let (x, y) = GlobalCursor::position();
        self.handles.tray_menu.popup(x, y);
    }

    fn update_autostart(&self) {
        let state = self.state.borrow();
        self.handles
            .tray_menu_autostart
            .set_checked(state.autostart.is_registered());
    }

    fn toggle_autostart(&self) {
        let state = self.state.borrow();

        if self.handles.tray_menu_autostart.checked() {
            state.autostart.remove();
        } else {
            state.autostart.register();
        }

        self.update_autostart();
    }

    fn toggle_debug(&self) {
        if self.handles.tray_menu_debug.checked() {
            unsafe { FreeConsole() };
            self.handles.tray_menu_debug.set_checked(false);
        } else {
            unsafe {
                AllocConsole();
                let console = GetConsoleWindow();
                let console_menu = GetSystemMenu(console, 0);
                DeleteMenu(console_menu, SC_CLOSE as _, MF_BYCOMMAND);
            }

            self.handles.tray_menu_debug.set_checked(true);
        }
    }

    fn toggle_disable(&self) {
        let mut state = self.state.borrow_mut();
        let hook = state.hook.as_mut().unwrap();

        if hook.active() {
            hook.disable();
            self.handles.tray.set_icon(&self.handles.icon_disabled);
            self.handles.tray_menu_disable.set_checked(true);
        } else {
            hook.enable();
            self.handles.tray.set_icon(&self.handles.icon_enabled);
            self.handles.tray_menu_disable.set_checked(false);
        }
    }
}

pub struct TrayIcon {
    data: Rc<TrayIconData>,
    handler: EventHandler,
    raw_handler: RawEventHandler,
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        native_windows_gui::unbind_event_handler(&self.handler);
        native_windows_gui::unbind_raw_event_handler(&self.raw_handler).unwrap();
    }
}

impl TrayIcon {
    pub fn new(console_available: bool, kb: &VirtualKeyboard) -> Result<Self, NwgError> {
        let state = State::new(kb.locked_layer().into());
        let mut data = TrayIconData::new(state);
        let handles = &mut data.handles;

        // Resources
        EmbedResource::builder().build(&mut handles.resources)?;

        Icon::builder()
            .source_embed(Some(&handles.resources))
            .source_embed_id(resources::ICON_KEYBOARD)
            .build(&mut handles.icon_enabled)?;

        Icon::builder()
            .source_embed(Some(&handles.resources))
            .source_embed_id(resources::ICON_KEYBOARD_DELETE)
            .build(&mut handles.icon_disabled)?;

        // Controls
        MessageWindow::builder().build(&mut handles.window)?;

        TrayNotification::builder()
            .icon(Some(&handles.icon_enabled))
            .tip(Some(&tooltip_message(kb.active_layer())))
            .parent(&handles.window)
            .build(&mut handles.tray)?;

        Menu::builder()
            .popup(true)
            .parent(&handles.window)
            .build(&mut handles.tray_menu)?;

        MenuItem::builder()
            .text("Run at system start")
            .parent(&handles.tray_menu)
            .build(&mut handles.tray_menu_autostart)?;

        MenuItem::builder()
            .text("Show debug output")
            .parent(&handles.tray_menu)
            .check(console_available)
            .disabled(console_available)
            .build(&mut handles.tray_menu_debug)?;

        MenuItem::builder()
            .text("Disable")
            .parent(&handles.tray_menu)
            .disabled(true)
            .build(&mut handles.tray_menu_disable)?;

        MenuItem::builder()
            .text("Exit")
            .parent(&handles.tray_menu)
            .build(&mut handles.tray_menu_exit)?;

        let data = Rc::new(data);

        let data_handler = Rc::downgrade(&data);
        let event_handler = move |evt, _evt_data, handle| {
            let data = data_handler.upgrade().unwrap();
            if evt != Event::OnMenuItemSelected {
                return;
            }

            if handle == data.handles.tray_menu_autostart {
                data.toggle_autostart();
            } else if handle == data.handles.tray_menu_debug {
                data.toggle_debug();
            } else if handle == data.handles.tray_menu_disable {
                data.toggle_disable();
            } else if handle == data.handles.tray_menu_exit {
                native_windows_gui::stop_thread_dispatch();
            }
        };
        let handler =
            native_windows_gui::full_bind_event_handler(&data.handles.window.handle, event_handler);

        // Use an additional low level handler, because high level handler does
        // not support double click events for the tray icon.
        let data_raw_handler = Rc::downgrade(&data);
        let raw_event_handler = move |_, msg, _, lparam| {
            let data = data_raw_handler.upgrade().unwrap();

            // Let’s hope `native-windows-gui` does not change internal constants.
            const NWG_TRAY: u32 = WM_USER + 102;
            if msg != NWG_TRAY {
                return None;
            }

            use winapi::um::winuser::*;
            match lparam as _ {
                WM_LBUTTONDBLCLK => data.toggle_disable(),
                WM_RBUTTONUP => {
                    data.update_autostart();
                    data.show_menu();
                }
                _ => (),
            }

            None
        };

        const TRAY_HANDLER_ID: usize = 0x74726179;
        let raw_handler = native_windows_gui::bind_raw_event_handler(
            &data.handles.window.handle,
            TRAY_HANDLER_ID,
            raw_event_handler,
        )?;

        Ok(Self {
            data,
            handler,
            raw_handler,
        })
    }

    pub fn set_hook(&self, hook: KeyboardHook) {
        self.data.state.borrow_mut().hook = Some(hook);
        self.data.handles.tray_menu_disable.set_enabled(true);
    }

    pub fn set_active_layer(&self, layer: &str) {
        self.data.handles.tray.set_tip(&tooltip_message(layer));
    }

    pub fn set_locked_layer(&self, layer: &str) {
        let mut state = self.data.state.borrow_mut();
        if state.locked_layer != layer {
            state.locked_layer = layer.into();
            self.data
                .handles
                .tray
                .show(&format!("Layer \"{layer}\" locked"), None, None, None);
        }
    }
}

fn tooltip_message(layer: &str) -> String {
    format!("Active Layer:\n{layer}")
}
