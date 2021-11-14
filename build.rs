const MENU: &str = r#"
10 MENU {
    POPUP "trayicon" {
        MENUITEM "&Exit", 11
    }
}
"#;

fn main() {
    winres::WindowsResource::new()
        .set_icon_with_id("icons/keyboard.ico", "1") // icon for the .exe file
        .set_icon_with_id("icons/keyboard_delete.ico", "2")
        .append_rc_content(MENU)
        .compile()
        .unwrap();
}
