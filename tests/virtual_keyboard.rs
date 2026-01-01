use kbremap::KeyAction::*;
use kbremap::{Layout, VirtualKeyboard};

#[test]
fn layer_activation() {
    let mut layout = Layout::new();
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let a = layout.add_layer();
    let b = layout.add_layer();
    let c = layout.add_layer();
    layout.add_modifier(0x11, base, a);
    layout.add_key(0x11, base, Ignore);
    layout.add_modifier(0x12, base, b);
    layout.add_key(0x12, base, Ignore);
    layout.add_key(0x20, base, Character('0'));
    layout.add_modifier(0x12, a, c);
    layout.add_key(0x12, a, Ignore);
    layout.add_key(0x20, a, Character('1'));
    layout.add_key(0x20, b, Character('2'));
    layout.add_key(0x20, c, Character('3'));

    let mut kb = VirtualKeyboard::new(layout);

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
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let shift = layout.add_layer();
    layout.add_modifier(0x2A, base, shift);
    layout.add_key(0x2A, base, VirtualKey(0xA0));
    layout.add_modifier(0xE036, base, shift);
    layout.add_key(0xE036, base, VirtualKey(0xA1));

    let mut kb = VirtualKeyboard::new(layout);

    assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0xA1)));
    assert_eq!(kb.press_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0xA1)));
}

#[test]
fn masked_modifier_on_base_layer() {
    let mut layout = Layout::new();
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let a = layout.add_layer();
    let b = layout.add_layer();
    let c = layout.add_layer();
    layout.add_modifier(0x0A, base, a);
    layout.add_key(0x0A, base, Ignore);
    layout.add_modifier(0x0B, base, b);
    layout.add_key(0x0B, base, Ignore);
    layout.add_modifier(0x0C, a, c);
    layout.add_key(0x0C, a, Ignore);
    layout.add_key(0xBB, b, Character('B'));
    layout.add_key(0xCC, c, Character('C')); // not reachable from base

    let mut kb = VirtualKeyboard::new(layout);

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
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let a = layout.add_layer();
    let b = layout.add_layer();
    let c = layout.add_layer();

    layout.add_modifier(0x0A, base, a);
    layout.add_key(0x0A, base, Ignore);
    layout.add_modifier(0xA0, base, a);
    layout.add_key(0xA0, base, Ignore);
    layout.add_modifier(0x0B, base, b);
    layout.add_key(0x0B, base, Ignore);
    layout.add_modifier(0xB0, base, b);
    layout.add_key(0xB0, base, Ignore);

    layout.add_key(0xFF, base, Character('X'));

    layout.add_modifier(0x0B, a, c);
    layout.add_key(0x0B, a, Ignore);
    layout.add_modifier(0xB0, a, c);
    layout.add_key(0xB0, a, Ignore);

    layout.add_layer_lock(0x0A, a, a);
    layout.add_modifier(0x0A, a, base);
    layout.add_layer_lock(0xA0, a, a);
    layout.add_modifier(0xA0, a, base);

    layout.add_key(0xFF, a, Character('A'));

    layout.add_modifier(0x0A, b, c);
    layout.add_key(0x0A, b, Ignore);
    layout.add_modifier(0xA0, b, c);
    layout.add_key(0xA0, b, Ignore);
    layout.add_layer_lock(0x0B, b, b);
    layout.add_layer_lock(0xB0, b, b);

    layout.add_key(0xFF, b, Character('B'));

    layout.add_layer_lock(0x0A, c, c);
    layout.add_layer_lock(0xA0, c, c);
    layout.add_layer_lock(0x0B, c, c);
    layout.add_layer_lock(0xB0, c, c);

    layout.add_key(0xFF, c, Character('C'));

    let mut kb = VirtualKeyboard::new(layout);

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

    // Try to lock layer c
    assert_eq!(kb.press_key(0xB0), Some(Ignore));
    assert_eq!(kb.release_key(0xB0), Some(Ignore));

    // Locks failed, on layer c because mod still pressed
    assert_eq!(kb.press_key(0xFF), Some(Character('C')));
    assert_eq!(kb.release_key(0xFF), Some(Character('C')));

    // Lock failed, on layer a still after mod released
    assert_eq!(kb.release_key(0x0B), Some(Ignore));
    assert_eq!(kb.press_key(0xFF), Some(Character('A')));
    assert_eq!(kb.release_key(0xFF), Some(Character('A')));

    // Unlock layer a
    assert_eq!(kb.press_key(0xA0), Some(Ignore));
    assert_eq!(kb.press_key(0x0A), Some(Ignore));
    assert_eq!(kb.release_key(0x0A), Some(Ignore));
    assert_eq!(kb.release_key(0xA0), Some(Ignore));

    // Check if locked to layer base
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));
}

#[test]
fn transparency() {
    let mut layout = Layout::new();
    let a = layout.add_layer();
    layout.set_base_layer(a);
    let b = layout.add_layer();
    let c = layout.add_layer();

    layout.add_modifier(0xAB, a, b);
    layout.add_key(0xAB, a, Ignore);
    layout.add_key(0x01, a, Character('A'));
    layout.add_key(0x02, a, Character('A'));
    layout.add_key(0x03, a, Character('A'));
    layout.add_modifier(0xBC, b, c);
    layout.add_key(0xBC, b, Ignore);
    layout.add_key(0x01, b, Character('B'));
    layout.add_key(0x02, b, Character('B'));
    layout.add_layer_lock(0xCC, c, c);
    layout.add_key(0xCC, c, Ignore);
    layout.add_key(0x01, c, Character('C'));
    layout.add_key(0x04, c, Character('C'));

    let mut kb = VirtualKeyboard::new(layout);

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

    // Unlock layer c
    assert_eq!(kb.press_key(0xAB), Some(Ignore));
    assert_eq!(kb.press_key(0xBC), Some(Ignore));
    assert_eq!(kb.press_key(0xCC), Some(Ignore));
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
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let a = layout.add_layer();
    let b = layout.add_layer();
    let c = layout.add_layer();
    let d = layout.add_layer();

    layout.add_modifier(0x0A, base, a);
    layout.add_key(0x0A, base, Ignore);
    layout.add_modifier(0xA0, base, a);
    layout.add_key(0xA0, base, Ignore);
    layout.add_modifier(0xAB, a, b);
    layout.add_key(0xAB, a, Ignore);
    layout.add_modifier(0xAC, a, c);
    layout.add_key(0xAC, a, Ignore);
    layout.add_modifier(0xBD, b, d);
    layout.add_key(0xBD, b, Ignore);
    layout.add_modifier(0xCD, c, d);
    layout.add_key(0xCD, c, Ignore);
    layout.add_layer_lock(0xBD, d, d);
    layout.add_layer_lock(0xCD, d, d);
    layout.add_key(0xFF, d, Character('X'));

    let mut kb = VirtualKeyboard::new(layout);

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
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let shift = layout.add_layer();

    // base layer
    layout.add_layer_lock(0x3A, base, shift); // caps lock
    layout.add_key(0x3A, base, VirtualKey(0x14)); // forward caps vk
    layout.add_key(0xFF, base, Character('x'));

    // shift layer
    layout.add_key(0xFF, shift, Character('X'));

    let mut kb = VirtualKeyboard::new(layout);

    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));

    // Activate caps lock (but do not release yet)
    assert_eq!(kb.press_key(0x3A), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));

    // Release caps lock key, shift layer stays activated
    assert_eq!(kb.release_key(0x3A), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0xFF), Some(Character('X')));
    assert_eq!(kb.release_key(0xFF), Some(Character('X')));

    // Deativate caps lock (but do not release yet)
    assert_eq!(kb.press_key(0x3A), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));

    // Release caps lock key
    assert_eq!(kb.release_key(0x3A), Some(VirtualKey(0x14)));
    assert_eq!(kb.press_key(0xFF), Some(Character('x')));
    assert_eq!(kb.release_key(0xFF), Some(Character('x')));
}

#[test]
fn layer_lock_caps_neo() {
    let mut layout = Layout::new();
    let base = layout.add_layer();
    layout.set_base_layer(base);
    let shift = layout.add_layer();

    layout.add_modifier(0x2A, base, shift);
    layout.add_key(0x2A, base, VirtualKey(0xA0)); // forward shift vk
    layout.add_modifier(0xE036, base, shift);
    layout.add_key(0xE036, base, VirtualKey(0xA1)); // forward shift vk

    layout.add_key(0xFF, base, Character('x'));

    layout.add_key(0x2A, shift, VirtualKey(0x14)); // caps lock vk
    layout.add_modifier(0x2A, shift, base); // temp base layer
    layout.add_layer_lock(0x2A, shift, shift);
    layout.add_key(0xE036, shift, VirtualKey(0x14)); // caps lock vk
    layout.add_modifier(0xE036, shift, base); // temp base layer
    layout.add_layer_lock(0xE036, shift, shift);

    layout.add_key(0xFF, shift, Character('X'));

    let mut kb = VirtualKeyboard::new(layout);

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
    let layout = Layout::new();
    assert!(!layout.is_valid());
}
