// `build.rs` uses the `winres` to create dynamically create a windows resource
// file (`.rc`) with icon and menu definitions.
// `main.rs` uses these constants to load the resources during runttime.

pub const ICON_KEYBOARD: usize = 1;
pub const ICON_KEYBOARD_DELETE: usize = 2;
