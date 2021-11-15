#[path = "src/resources.rs"]
mod resources;

use fstrings::*;
use resources::*;

fn main() {
    winres::WindowsResource::new()
        .set_icon_with_id("icons/keyboard.ico", &f!("{ICON_KEYBOARD}")) // icon for the .exe file
        .set_icon_with_id("icons/keyboard_delete.ico", &f!("{ICON_KEYBOARD_DELETE}"))
        .append_rc_content(&f!(r#"
{MENU} MENU
BEGIN
    POPUP "trayicon"
    BEGIN
        MENUITEM "Exit", {MENU_EXIT}
    END
END"#))
        .compile()
        .unwrap();
}
