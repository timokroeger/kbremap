#[path = "src/resources.rs"]
mod resources;

use std::env;

use resources::*;

const MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <assemblyIdentity type="win32" name="?NAME?" version="?VERSION?" />
    <trustInfo xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="asInvoker" />
            </requestedPrivileges>
        </security>
    </trustInfo>
    <application xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
        <windowsSettings>
            <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
            <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">permonitorv2</dpiAwareness>
            <heapType xmlns="http://schemas.microsoft.com/SMI/2020/WindowsSettings">SegmentHeap</heapType>
        </windowsSettings>
    </application>
</assembly>"#;

fn main() {
    // Update manifest with package name and version from Cargo.toml.
    let name = env::var("CARGO_PKG_NAME").unwrap();
    let version = [
        env::var("CARGO_PKG_VERSION_MAJOR").unwrap(),
        env::var("CARGO_PKG_VERSION_MINOR").unwrap(),
        env::var("CARGO_PKG_VERSION_PATCH").unwrap(),
        "0".to_string(),
    ]
    .join(".");

    let manifest = MANIFEST.to_string();
    let manifest = manifest.replace("?NAME?", &name);
    let manifest = manifest.replace("?VERSION?", &version);

    // Minimize XML
    let manifest = manifest.replace("\r", "");
    let manifest = manifest.replace("\n", "");
    let manifest = manifest.replace("    ", "");

    winres::WindowsResource::new()
        .set_manifest(&manifest)
        .set_icon_with_id("icons/keyboard.ico", &format!("{}", ICON_KEYBOARD)) // icon for the .exe file
        .set_icon_with_id(
            "icons/keyboard_delete.ico",
            &format!("{}", ICON_KEYBOARD_DELETE),
        )
        .compile()
        .unwrap();
}
