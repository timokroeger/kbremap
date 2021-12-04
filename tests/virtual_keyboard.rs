use kbremap::layout::KeyAction::*;
use kbremap::layout::LayoutBuilder;
use kbremap::virtual_keyboard::VirtualKeyboard;

#[test]
fn layer_activation() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x11, "base", "l1", None)
        .add_modifier(0x12, "base", "l2", None)
        .add_key(0x20, "base", Character('0'))
        .add_modifier(0x12, "l1", "l3", None)
        .add_key(0x20, "l1", Character('1'))
        .add_key(0x20, "l2", Character('2'))
        .add_key(0x20, "l3", Character('3'));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}

#[test]
fn accidental_shift_lock_issue25() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x2A, "base", "shift", Some(0xA0))
        .add_modifier(0xE036, "base", "shift", Some(0xA1));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

    assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0xA1)));
    assert_eq!(kb.press_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0x002A), Some(VirtualKey(0xA0)));
    assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0xA1)));

    Ok(())
}

#[test]
fn cyclic_layers() {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x0001, "base", "overlay", None)
        .add_modifier(0x0002, "overlay", "base", None);
    let layout = layout.build();

    assert!(VirtualKeyboard::new(layout).is_err());
}

#[test]
fn masked_modifier_on_base_layer() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x0A, "base", "a", None)
        .add_modifier(0x0B, "base", "b", None)
        .add_modifier(0x0C, "a", "c", None)
        .add_key(0xBB, "b", Character('B'))
        .add_key(0xCC, "c", Character('C')); // not reachable from base
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}

#[test]
fn layer_lock() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x0A, "base", "a", None)
        .add_modifier(0xA0, "base", "a", None)
        .add_modifier(0x0B, "base", "b", None)
        .add_modifier(0xB0, "base", "b", None)
        .add_key(0xFF, "base", Character('X'))
        .add_modifier(0x0B, "a", "c", None)
        .add_modifier(0xB0, "a", "c", None)
        .add_layer_lock(0x0A, "a", "a", None)
        .add_layer_lock(0xA0, "a", "a", None)
        .add_key(0xFF, "a", Character('A'))
        .add_modifier(0x0A, "b", "c", None)
        .add_modifier(0xA0, "b", "c", None)
        .add_layer_lock(0x0B, "b", "b", None)
        .add_layer_lock(0xB0, "b", "b", None)
        .add_key(0xFF, "b", Character('B'))
        .add_layer_lock(0x0A, "c", "c", None)
        .add_layer_lock(0xA0, "c", "c", None)
        .add_layer_lock(0x0B, "c", "c", None)
        .add_layer_lock(0xB0, "c", "c", None)
        .add_key(0xFF, "c", Character('C'));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}

#[test]
fn transparency() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0xAB, "a", "b", None)
        .add_key(0x01, "a", Character('A'))
        .add_key(0x02, "a", Character('A'))
        .add_key(0x03, "a", Character('A'))
        .add_modifier(0xBC, "b", "c", None)
        .add_key(0x01, "b", Character('B'))
        .add_key(0x02, "b", Character('B'))
        .add_layer_lock(0xCC, "c", "c", None)
        .add_key(0x01, "c", Character('C'))
        .add_key(0x04, "c", Character('C'));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}

#[test]
fn layer_lock_shared_path() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x0A, "base", "a", None)
        .add_modifier(0xA0, "base", "a", None)
        .add_modifier(0xAB, "a", "b", None)
        .add_modifier(0xAC, "a", "c", None)
        .add_modifier(0xBD, "b", "d", None)
        .add_modifier(0xCD, "c", "d", None)
        .add_layer_lock(0x0A, "d", "d", None)
        .add_layer_lock(0xAB, "d", "d", None)
        .add_layer_lock(0xBD, "d", "d", None)
        .add_layer_lock(0xA0, "d", "d", None)
        .add_layer_lock(0xAC, "d", "d", None)
        .add_layer_lock(0xCD, "d", "d", None)
        .add_key(0xFF, "d", Character('X'));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}

#[test]
fn layer_lock_caps() -> anyhow::Result<()> {
    let mut layout = LayoutBuilder::new();
    layout
        .add_modifier(0x2A, "base", "shift", Some(0xA0)) // forward shift vk
        .add_modifier(0xE036, "base", "shift", Some(0xA0)) // forward shift vk
        .add_key(0xFF, "base", Character('x'))
        .add_layer_lock(0x2A, "shift", "shift", Some(0x14)) // caps lock vk
        .add_layer_lock(0xE036, "shift", "shift", Some(0x14)) // caps lock vk
        .add_key(0xFF, "shift", Character('X'));
    let layout = layout.build();
    let mut kb = VirtualKeyboard::new(layout)?;

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

    Ok(())
}
