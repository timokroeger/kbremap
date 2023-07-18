// `build.rs` uses the `winres` to create dynamically create a windows resource
// file (`.rc`) with icon and menu definitions.
// `main.rs` uses these constants to load the resources during runtime.

pub const ICON_KEYBOARD: u16 = 1;
pub const ICON_KEYBOARD_DELETE: u16 = 2;
