# Changelog

## [Unreleased]

### Added
- Layer locking #32
- Make layer locking configurable

### Changed
- Make layers transparent. When a key has no action defined, check the previous layer. #34

### Fixed
- The base layer can now have any name

## [1.2.0] - 2021-11-21

### Added
- Option to run at system startup, configurable from the tray icon popup menu
- Load configuration file from executable directory if now found in current working directory

### Changed
- Use `native-windows-gui` crate for the tray icon menu

### Fixed
- Accidental shift lock #25
- A problem where windows disabled the keyboard hook when debug output was enabled

### Removed
- `debug_output` configuration. Debug output can be toggled from the tray icon menu

## [1.1.0] - 2021-11-17

### Added
- `disable_caps_lock` config option to prevent accidental caps lock from RDP usage
- Disable entry in tray icon popup menu

### Changed
- Dvorak layout in the example config
- Enable and disable with double-click on tray icon instead of single-click
- Use the WinAPI directly to create the tray icon and context menu

### Fixed
- Invalid config entries in the `neo.toml` file


## [1.0.0] - 2020-12-03

Initial Release


[Unreleased]: https://github.com/timokroeger/kbremap/compare/v1.1.0..HEAD
[1.1.0]: https://github.com/timokroeger/kbremap/compare/v1.0.0..v1.1.0
[1.0.0]: https://github.com/timokroeger/kbremap/releases/tag/v1.0.0
