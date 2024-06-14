use kbremap::KeyAction::*;
use kbremap::{Error, Layout, VirtualKeyboard};

#[test]
fn layer_activation() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let a = layout.add_layer(String::from("a"));
    let b = layout.add_layer(String::from("b"));
    let c = layout.add_layer(String::from("c"));
    layout.add_modifier(0x11, base, a, None);
    layout.add_modifier(0x12, base, b, None);
    layout.add_key(0x20, base, Character('0'));
    layout.add_modifier(0x12, a, c, None);
    layout.add_key(0x20, a, Character('1'));
    layout.add_key(0x20, b, Character('2'));
    layout.add_key(0x20, c, Character('3'));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // L0
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));

    // L1
    assert_eq!(kb.press_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));

    // L2
    assert_eq!(kb.press_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));

    // L1 -> L3 -> L2
    assert_eq!(kb.press_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x20), Some(Character('1')));
    assert_eq!(kb.press_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('3')));
    assert_eq!(kb.release_key(0x20), Some(Character('3')));
    assert_eq!(kb.release_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));

    // L2 -> XX (L2 still active) -> L1
    assert_eq!(kb.press_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x20), Some(Character('2')));
    assert_eq!(kb.press_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x20), Some(Character('2')));
    assert_eq!(kb.release_key(0x12), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));

    // Change layer during key press
    assert_eq!(kb.press_key(0x11), Some(Ignore));
    assert_eq!(kb.press_key(0x20), Some(Character('1')));
    assert_eq!(kb.release_key(0x11), Some(Ignore));
    assert_eq!(kb.release_key(0x20), Some(Character('1')));
    assert_eq!(kb.press_key(0x20), Some(Character('0')));
    assert_eq!(kb.release_key(0x20), Some(Character('0')));
}

#[test]
fn accidental_shift_lock_issue25() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let shift = layout.add_layer(String::from("shift"));
    layout.add_modifier(0x2A, base, shift, Some(0xA0));
    layout.add_modifier(0xE036, base, shift, Some(0xA1));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0xA1)));
    assert_eq!(kb.press_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0xA1)));
}

#[test]
fn cyclic_layers() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let shift = layout.add_layer(String::from("shift"));
    layout.add_modifier(0x0001, base, shift, None);
    layout.add_modifier(0x0002, shift, base, None);
    assert_eq!(layout.finalize().unwrap_err(), Error::CycleInGraph);

    VirtualKeyboard::new(&layout);
}

#[test]
fn masked_modifier_on_base_layer() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let a = layout.add_layer(String::from("a"));
    let b = layout.add_layer(String::from("b"));
    let c = layout.add_layer(String::from("c"));
    layout.add_modifier(0x0A, base, a, None);
    layout.add_modifier(0x0B, base, b, None);
    layout.add_modifier(0x0C, a, c, None);
    layout.add_key(0xBB, b, Character('B'));
    layout.add_key(0xCC, c, Character('C')); // not reachable from base
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // "B" does not exist on base layer
    assert_eq!(kb.press_key(0xBB), None);
    assert_eq!(kb.release_key(0xBB), None);

    // Layer c should not be activated from the base layer
    assert_eq!(kb.press_key(0x0C), None);
    assert_eq!(kb.press_key(0xCC), None);
    assert_eq!(kb.release_key(0xCC), None);

    // But Layer b should be activated even when modifier for layer c pressed.
    assert_eq!(kb.press_key(0x0B), Some(Ignore));
    assert_eq!(kb.press_key(0xBB), Some(Character('B')));
    assert_eq!(kb.release_key(0xBB), Some(Character('B')));

    // Release layer c key (it was never activated) and make sure we are still on layer b.
    assert_eq!(kb.release_key(0x0C), None);
    assert_eq!(kb.press_key(0xBB), Some(Character('B')));
    assert_eq!(kb.release_key(0xBB), Some(Character('B')));

    // Release layer b key
    assert_eq!(kb.release_key(0x0B), Some(Ignore));

    // "B" does not exist on base layer
    assert_eq!(kb.press_key(0xBB), None);
    assert_eq!(kb.release_key(0xBB), None);
}

#[test]
fn layer_lock() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let a = layout.add_layer(String::from("a"));
    let b = layout.add_layer(String::from("b"));
    let c = layout.add_layer(String::from("c"));
    layout.add_modifier(0x0A, base, a, None);
    layout.add_modifier(0xA0, base, a, None);
    layout.add_modifier(0x0B, base, b, None);
    layout.add_modifier(0xB0, base, b, None);
    layout.add_key(0xFF, base, Character('X'));
    layout.add_modifier(0x0B, a, c, None);
    layout.add_modifier(0xB0, a, c, None);
    layout.add_layer_lock(0x0A, a, a, None);
    layout.add_layer_lock(0xA0, a, a, None);
    layout.add_key(0xFF, a, Character('A'));
    layout.add_modifier(0x0A, b, c, None);
    layout.add_modifier(0xA0, b, c, None);
    layout.add_layer_lock(0x0B, b, b, None);
    layout.add_layer_lock(0xB0, b, b, None);
    layout.add_key(0xFF, b, Character('B'));
    layout.add_layer_lock(0x0A, c, c, None);
    layout.add_layer_lock(0xA0, c, c, None);
    layout.add_layer_lock(0x0B, c, c, None);
    layout.add_layer_lock(0xB0, c, c, None);
    layout.add_key(0xFF, c, Character('C'));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // Lock layer a
    assert_eq!(kb.press_key(0x0A), Some(Ignore));
    assert_eq!(kb.press_key(0xA0), Some(Ignore));
    assert_eq!(kb.release_key(0x0A), Some(Ignore));
    assert_eq!(kb.release_key(0xA0), Some(Ignore));

    // Test if locked
    assert_eq!(kb.press_key(0xFF), Some(Character('A')));
    assert_eq!(kb.release_key(0xFF), Some(Character('A')));

    // Temp switch back to layer base
    assert_eq!(kb.press_key(0x0A), Some(Ignore));
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0x0A), Some(Ignore));

    // Temp switch to layer c
    assert_eq!(kb.press_key(0x0B), Some(Ignore));
    assert_eq!(kb.press_key(0xFF), Some(Character('C')));
    assert_eq!(kb.release_key(0xFF), Some(Character('C')));

    // Lock layer c
    assert_eq!(kb.press_key(0xB0), Some(Ignore));
    assert_eq!(kb.release_key(0xB0), Some(Ignore));

    // Temp switched to layer a still
    assert_eq!(kb.press_key(0xFF), Some(Character('A')));
    assert_eq!(kb.release_key(0xFF), Some(Character('A')));

    // Check if locked to layer c
    assert_eq!(kb.release_key(0x0B), Some(Ignore));
    assert_eq!(kb.press_key(0xFF), Some(Character('C')));
    assert_eq!(kb.release_key(0xFF), Some(Character('C')));

    // Unlock layer c
    assert_eq!(kb.press_key(0xA0), Some(Ignore));
    assert_eq!(kb.press_key(0xB0), Some(Ignore));
    assert_eq!(kb.press_key(0x0A), Some(Ignore));
    assert_eq!(kb.release_key(0x0A), Some(Ignore));
    assert_eq!(kb.release_key(0xB0), Some(Ignore));
    assert_eq!(kb.release_key(0xA0), Some(Ignore));

    // Check if locked to layer base
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));
}

#[test]
fn transparency() {
    let mut layout = Layout::new();
    let a = layout.add_layer(String::from("a"));
    let b = layout.add_layer(String::from("b"));
    let c = layout.add_layer(String::from("c"));
    layout.add_modifier(0xAB, a, b, None);
    layout.add_key(0x01, a, Character('A'));
    layout.add_key(0x02, a, Character('A'));
    layout.add_key(0x03, a, Character('A'));
    layout.add_modifier(0xBC, b, c, None);
    layout.add_key(0x01, b, Character('B'));
    layout.add_key(0x02, b, Character('B'));
    layout.add_layer_lock(0xCC, c, c, None);
    layout.add_key(0x01, c, Character('C'));
    layout.add_key(0x04, c, Character('C'));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // Layer a
    assert_eq!(kb.press_key(0x01), Some(Character('A')));
    assert_eq!(kb.release_key(0x01), Some(Character('A')));
    assert_eq!(kb.press_key(0x02), Some(Character('A')));
    assert_eq!(kb.release_key(0x02), Some(Character('A')));
    assert_eq!(kb.press_key(0x03), Some(Character('A')));
    assert_eq!(kb.release_key(0x03), Some(Character('A')));
    assert_eq!(kb.press_key(0x04), None);
    assert_eq!(kb.release_key(0x04), None);

    assert_eq!(kb.press_key(0xAB), Some(Ignore));

    // Layer b
    assert_eq!(kb.press_key(0x01), Some(Character('B')));
    assert_eq!(kb.release_key(0x01), Some(Character('B')));
    assert_eq!(kb.press_key(0x02), Some(Character('B')));
    assert_eq!(kb.release_key(0x02), Some(Character('B')));
    assert_eq!(kb.press_key(0x03), Some(Character('A')));
    assert_eq!(kb.release_key(0x03), Some(Character('A')));
    assert_eq!(kb.press_key(0x04), None);
    assert_eq!(kb.release_key(0x04), None);

    assert_eq!(kb.press_key(0xBC), Some(Ignore));

    // Layer c
    assert_eq!(kb.press_key(0x01), Some(Character('C')));
    assert_eq!(kb.release_key(0x01), Some(Character('C')));
    assert_eq!(kb.press_key(0x02), Some(Character('B')));
    assert_eq!(kb.release_key(0x02), Some(Character('B')));
    assert_eq!(kb.press_key(0x03), Some(Character('A')));
    assert_eq!(kb.release_key(0x03), Some(Character('A')));
    assert_eq!(kb.press_key(0x04), Some(Character('C')));
    assert_eq!(kb.release_key(0x04), Some(Character('C')));

    // Lock layer c
    assert_eq!(kb.press_key(0xCC), Some(Ignore));
    assert_eq!(kb.release_key(0xCC), Some(Ignore));
    assert_eq!(kb.release_key(0xBC), Some(Ignore));
    assert_eq!(kb.release_key(0xAB), Some(Ignore));

    // Layer c
    assert_eq!(kb.press_key(0x01), Some(Character('C')));
    assert_eq!(kb.release_key(0x01), Some(Character('C')));
    assert_eq!(kb.press_key(0x02), Some(Character('B')));
    assert_eq!(kb.release_key(0x02), Some(Character('B')));
    assert_eq!(kb.press_key(0x03), Some(Character('A')));
    assert_eq!(kb.release_key(0x03), Some(Character('A')));
    // Should be transparent to layer c now
    assert_eq!(kb.press_key(0x04), Some(Character('C')));
    assert_eq!(kb.release_key(0x04), Some(Character('C')));

    // Unlock layer c, with a different sequence
    assert_eq!(kb.press_key(0xCC), Some(Ignore));
    assert_eq!(kb.press_key(0xAB), Some(Ignore));
    assert_eq!(kb.press_key(0xBC), Some(Ignore));
    assert_eq!(kb.release_key(0xCC), Some(Ignore));
    assert_eq!(kb.release_key(0xAB), Some(Ignore));
    assert_eq!(kb.release_key(0xBC), Some(Ignore));

    assert_eq!(kb.press_key(0x01), Some(Character('A')));
    assert_eq!(kb.release_key(0x01), Some(Character('A')));
    assert_eq!(kb.press_key(0x02), Some(Character('A')));
    assert_eq!(kb.release_key(0x02), Some(Character('A')));
    assert_eq!(kb.press_key(0x03), Some(Character('A')));
    assert_eq!(kb.release_key(0x03), Some(Character('A')));
    assert_eq!(kb.press_key(0x04), None);
    assert_eq!(kb.release_key(0x04), None);
}

#[test]
fn layer_lock_shared_path() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let a = layout.add_layer(String::from("a"));
    let b = layout.add_layer(String::from("b"));
    let c = layout.add_layer(String::from("c"));
    let d = layout.add_layer(String::from("d"));
    layout.add_modifier(0x0A, base, a, None);
    layout.add_modifier(0xA0, base, a, None);
    layout.add_modifier(0xAB, a, b, None);
    layout.add_modifier(0xAC, a, c, None);
    layout.add_modifier(0xBD, b, d, None);
    layout.add_modifier(0xCD, c, d, None);
    layout.add_layer_lock(0xBD, d, d, None);
    layout.add_layer_lock(0xCD, d, d, None);
    layout.add_key(0xFF, d, Character('X'));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // Just make sure it does not panic.
    kb.press_key(0x0A);
    kb.press_key(0xAB);
    kb.press_key(0xBD);
    kb.press_key(0xA0);
    kb.press_key(0xAC);
    kb.press_key(0xCD);
    kb.release_key(0x0A);
    kb.release_key(0xAB);
    kb.release_key(0xBD);
    kb.release_key(0xA0);
    kb.release_key(0xAC);
    kb.release_key(0xCD);

    // Check if locked
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));
}

#[test]
fn layer_lock_caps() {
    let mut layout = Layout::new();
    let base = layout.add_layer(String::from("base"));
    let shift = layout.add_layer(String::from("shift"));
    layout.add_modifier(0x2A, base, shift, Some(0xA0)); // forward shift vk
    layout.add_modifier(0xE036, base, shift, Some(0xA0)); // forward shift vk
    layout.add_key(0xFF, base, Character('x'));
    layout.add_layer_lock(0x2A, shift, shift, Some(0x14)); // caps lock vk
    layout.add_layer_lock(0xE036, shift, shift, Some(0x14)); // caps lock vk
    layout.add_key(0xFF, shift, Character('X'));
    layout.finalize().unwrap();

    let mut kb = VirtualKeyboard::new(&layout);

    // base layer
    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));

    // activate caps lock
    assert_eq!(kb.press_key(0x2A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));

    // temp base layer
    assert_eq!(kb.release_key(0x2A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0x14)));

    // locked shift layer
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));

    // deactivate caps lock
    assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0x2A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0x2A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0x14)));

    // base layer
    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));
}

#[test]
fn empty_configuration() {
    let mut layout = Layout::new();
    assert_eq!(layout.finalize().unwrap_err(), Error::EmptyConfiguration);
}
