use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui::{
    EmbedResource, Event, EventHandler, GlobalCursor, Icon, Menu, MenuItem, MessageWindow,
    NwgError, RawEventHandler, TrayNotification,
};

use crate::resources;

#[derive(Default)]
struct State {
    disabled: bool,
}

#[derive(Default)]
struct TrayIconData {
    resources: EmbedResource,
    icon_enabled: Icon,
    icon_disabled: Icon,
    window: MessageWindow,
    tray: TrayNotification,
    tray_menu: Menu,
    tray_menu_disable: MenuItem,
    tray_menu_exit: MenuItem,
    state: RefCell<State>,
}

impl TrayIconData {
    fn show_menu(&self) {
        let (x, y) = GlobalCursor::position();
        self.tray_menu.popup(x, y);
    }

    fn toggle_disable(&self) {
        let mut state = self.state.borrow_mut();
        if state.disabled {
            state.disabled = false;
            self.tray.set_icon(&self.icon_enabled);
            self.tray_menu_disable.set_checked(false);
        } else {
            state.disabled = true;
            self.tray.set_icon(&self.icon_disabled);
            self.tray_menu_disable.set_checked(true);
        }
    }

    fn exit(&self) {
        native_windows_gui::stop_thread_dispatch();
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
    pub fn new() -> Result<Self, NwgError> {
        let mut data = TrayIconData::default();

        // Resources
        EmbedResource::builder().build(&mut data.resources)?;

        Icon::builder()
            .source_embed(Some(&data.resources))
            .source_embed_id(resources::ICON_KEYBOARD)
            .size(Some((0, 0))) // makes the icon less blury
            .build(&mut data.icon_enabled)?;

        Icon::builder()
            .source_embed(Some(&data.resources))
            .source_embed_id(resources::ICON_KEYBOARD_DELETE)
            .size(Some((0, 0))) // makes the icon less blury
            .build(&mut data.icon_disabled)?;

        // Controls
        MessageWindow::builder().build(&mut data.window)?;

        TrayNotification::builder()
            .icon(Some(&data.icon_enabled))
            .parent(&data.window)
            .build(&mut data.tray)?;

        Menu::builder()
            .popup(true)
            .parent(&data.window)
            .build(&mut data.tray_menu)?;

        MenuItem::builder()
            .text("Disable")
            .parent(&data.tray_menu)
            .build(&mut data.tray_menu_disable)?;

        MenuItem::builder()
            .text("Exit")
            .parent(&data.tray_menu)
            .build(&mut data.tray_menu_exit)?;

        let data = Rc::new(data);

        let data_handler = Rc::downgrade(&data);
        let event_handler = move |evt, _evt_data, handle| {
            let data = data_handler.upgrade().unwrap();
            if evt != Event::OnMenuItemSelected {
                return;
            }

            if handle == data.tray_menu_disable {
                data.toggle_disable();
            } else if handle == data.tray_menu_exit {
                data.exit();
            }
        };
        let handler =
            native_windows_gui::full_bind_event_handler(&data.window.handle, event_handler);

        // Use an additional low level handler, because high level handler does
        // not support double click events for the tary icon.
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
                WM_RBUTTONUP => data.show_menu(),
                _ => (),
            }

            None
        };

        const TRAY_HANDLER_ID: usize = 0x74726179;
        let raw_handler = native_windows_gui::bind_raw_event_handler(
            &data.window.handle,
            TRAY_HANDLER_ID,
            raw_event_handler,
        )?;

        Ok(Self {
            data,
            handler,
            raw_handler,
        })
    }

    pub fn is_enabled(&self) -> bool {
        !self.data.state.borrow().disabled
    }
}
