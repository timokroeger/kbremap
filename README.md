# `kbremap`

Custom keyboard layouts for windows.

`config.toml` is a well commented example config for the Dvorak layout.

## Features
* Remap any key to any other key
* Supports Unicode characters, including most Emojis ⌨️🔥
* Virtual layers support (e.g. right alt to overlay arrow keys for navigation)
* No installation or administrator rights required
* Double-click on tray icon disables the layout
* Option to run at system startup
* Uses the windows low-level keyboard hook for maximum compatibility

## Features `neo.toml` configuration
* Supports all 6 layers of the [Neo-Layout](https://neo-layout.org/)
* Support for dead keys on L1 and L2 (PR for L3-L6 welcome)
* Optional QWERTY/QWERTZ layout for shortcuts with CTRL, ALT and WIN modifiers

## Known issues
* Not working for RDP in full screen mode (or when "Apply Windows key combinations:
  On the remote computer" is set). Using a second instance of kbremap on the remote
  machine works fine as workaround.
* Compose key not avaible (yet)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
