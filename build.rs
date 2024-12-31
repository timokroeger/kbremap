#[path = "src/resources.rs"]
mod resources;

#[cfg(not(feature = "runtime-config"))]
#[path = "src/config.rs"]
mod config;

#[cfg(not(feature = "runtime-config"))]
#[path = "src/layout.rs"]
#[allow(dead_code)]
mod layout;

use std::env;

use resources::*;
use winresource::WindowsResource;

const MANIFEST: &str = r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
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
</assembly>
"#;

const RC: &str = r#"
1 VERSIONINFO
    FILEVERSION ?MAJOR?, ?MINOR?, ?PATCH?, 0
    PRODUCTVERSION ?MAJOR?, ?MINOR?, ?PATCH?, 0
    FILEOS 0x40004
    FILETYPE 0x1
    {}
?ICON_KEYBOARD? ICON "icons/keyboard.ico"
?ICON_KEYBOARD_DELETE? ICON "icons/keyboard_delete.ico"
1 24 {"?MANIFEST?"}
"#;

fn main() -> anyhow::Result<()> {
    // Update manifest with package name and version from Cargo.toml.
    let name = env::var("CARGO_PKG_NAME")?;
    let major = env::var("CARGO_PKG_VERSION_MAJOR")?;
    let minor = env::var("CARGO_PKG_VERSION_MINOR")?;
    let patch = env::var("CARGO_PKG_VERSION_PATCH")?;
    let version = [&major, &minor, &patch, "0"].join(".");

    let manifest = MANIFEST.to_string();
    let manifest = manifest.replace("?NAME?", &name);
    let manifest = manifest.replace("?VERSION?", &version);

    // Minimize XML
    let manifest = manifest.replace('\r', "");
    let manifest = manifest.replace('\n', "");
    let manifest = manifest.replace("    ", "");

    let rc = RC.to_string();
    let rc = rc.replace("?MAJOR?", &major);
    let rc = rc.replace("?MINOR?", &minor);
    let rc = rc.replace("?PATCH?", &patch);
    let rc = rc.replace("?MANIFEST?", &manifest.replace('"', "\"\""));
    let rc = rc.replace("?ICON_KEYBOARD?", &ICON_KEYBOARD.to_string());
    let rc = rc.replace("?ICON_KEYBOARD_DELETE?", &ICON_KEYBOARD_DELETE.to_string());

    let out_dir = env::var("OUT_DIR")?;

    let rc_file = format!("{out_dir}/resource.rc");
    std::fs::write(&rc_file, rc)?;

    WindowsResource::new()
        .set_resource_file(&rc_file)
        .compile()?;

    #[cfg(not(feature = "runtime-config"))]
    {
        use config::{Config, ReadableConfig};
        use std::{
            fs::{self, File},
            io::Write,
            path::PathBuf,
        };

        let config_str = fs::read_to_string("config.toml")?;
        let config_toml = toml::from_str::<ReadableConfig>(&config_str)?;
        let config = Config::try_from(config_toml)?;
        let config_bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&config)?;

        let out_dir = &PathBuf::from(env::var("OUT_DIR")?);
        File::create(out_dir.join("config.bin"))?.write_all(&config_bytes)?;
    }

    Ok(())
}
