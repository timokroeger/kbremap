# Changelog

## [1.6.0] - 2023-07-19

- Allow configurations with only one layer
- Fix tray icon disappearing when `explorer.exe` restarts
- Use `windows-sys` crate to build the tray icon menu without any additional abstractions.
- Remove notifications on layer lock (it annoyed me over time).
- Remove tooltip with current layer (never used it).
- Disable any layer locks when re-enabling via double tray icon click or context menu.

## [1.5.1] - 2023-04-15

- Prevent crash when typing while text is selected in the debug console.
- Forward unknown key release events: Fixes stuck ALT key when switching from a window with elevated rights. 

## [1.5.0] - 2022-12-11

- The tray icon tooltip shows the active layer
- Improved font rendering on hidpi screens
- Unregister hook when disabled

## [1.4.2] - 2022-08-16

- Fix unsoundness issue in low level keyboard-hook
- Fix dead keys for neo layout example config
- Update dependencies

## [1.4.1] - 2022-04-11

- Removing `tracing` dependency
- Fix bug where the layer graph was wrong for some modifier configurations

## [1.4.0] - 2021-12-23

### Added
- Show a notification when a layer was locked

### Changed
- Do not allow running duplicate instances of the same binary file.

## [1.3.0] - 2021-12-04

### Added
- Layer locking #32
- Make layer locking configurable

### Changed
- Make layers transparent. When a key has no action defined, check the previous layer (#34).
- Caps lock can be assigned to a layer (#44) with the `caps_lock_layer` config.
  Replaces the `disable_caps_lock` config entry.

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


[Unreleased]: https://github.com/timokroeger/kbremap/compare/v1.6.0..HEAD
[1.6.0]: https://github.com/timokroeger/kbremap/compare/v1.5.1..v1.6.0
[1.5.1]: https://github.com/timokroeger/kbremap/compare/v1.5.0..v1.5.1
[1.5.0]: https://github.com/timokroeger/kbremap/compare/v1.4.2..v1.5.0
[1.4.2]: https://github.com/timokroeger/kbremap/compare/v1.4.1..v1.4.2
[1.4.1]: https://github.com/timokroeger/kbremap/compare/v1.4.0..v1.4.1
[1.4.0]: https://github.com/timokroeger/kbremap/compare/v1.3.0..v1.4.0
[1.3.0]: https://github.com/timokroeger/kbremap/compare/v1.2.0..v1.3.0
[1.2.0]: https://github.com/timokroeger/kbremap/compare/v1.1.0..v1.2.0
[1.1.0]: https://github.com/timokroeger/kbremap/compare/v1.0.0..v1.1.0
[1.0.0]: https://github.com/timokroeger/kbremap/releases/tag/v1.0.0
