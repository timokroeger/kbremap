# `kbremap`

Custom keyboard layouts for windows. Fully configurable for quick prototyping of new layouts.  
Configuration loosely inspired by custom keyboard firmwares like [QMK](https://qmk.fm/).

## Features
* Remap any key to any other key
* Support for Unicode characters
* Support for any number of virtual layers
* No installation or administrator rights required
* Uses the windows low-level keyboard hook for maximum compatibility

## Features `neo.toml` configuration
* Supports all 6 layers of the [Neo-Layout](https://neo-layout.org/)
* Support for dead keys on L1 and L2 (PR for L3-L6 welcome)
* Optional QWERTY/QWERTZ layout for shortcuts with CTRL, ALT and WIN modifiers

## Known issues
* Layer locking not supported
* Layer "base" must exists in the configuration

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
