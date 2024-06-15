#[path = "src/resources.rs"]
mod resources;

#[cfg(not(feature = "runtime-config"))]
#[path = "src/config.rs"]
mod config;

#[cfg(not(feature = "runtime-config"))]
#[path = "src/layout.rs"]
mod layout;

use std::env;

use resources::*;
use winresource::WindowsResource;

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
            <activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>
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
    let manifest = manifest.replace('\r', "");
    let manifest = manifest.replace('\n', "");
    let manifest = manifest.replace("    ", "");

    WindowsResource::new()
        .set_manifest(&manifest)
        .set_icon_with_id("icons/keyboard.ico", &format!("{}", ICON_KEYBOARD)) // icon for the .exe file
        .set_icon_with_id(
            "icons/keyboard_delete.ico",
            &format!("{}", ICON_KEYBOARD_DELETE),
        )
        .compile()
        .unwrap();

    #[cfg(not(feature = "runtime-config"))]
    {
        use config::{Config, ReadableConfig};
        use std::{
            fs::{self, File},
            io::Write,
            path::PathBuf,
        };

        let config_str = fs::read_to_string("config.toml").unwrap();
        let config: ReadableConfig = toml::from_str(&config_str).unwrap();
        let config = Config::try_from(config).unwrap();
        let config = postcard::to_stdvec(&config).unwrap();

        let out_dir = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
        File::create(out_dir.join("config.bin"))
            .unwrap()
            .write_all(&config)
            .unwrap();
    }
}
