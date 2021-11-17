#[path = "src/resources.rs"]
mod resources;

use resources::*;

fn main() {
    winres::WindowsResource::new()
        .set_icon_with_id("icons/keyboard.ico", &format!("{}", ICON_KEYBOARD)) // icon for the .exe file
        .set_icon_with_id(
            "icons/keyboard_delete.ico",
            &format!("{}", ICON_KEYBOARD_DELETE),
        )
        .compile()
        .unwrap();
}
