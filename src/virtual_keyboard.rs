//! Remapping and layer switching logic.

use crate::layout::{KeyAction, LayerIdx, Layout, ScanCode};

const BASE_LAYER: LayerIdx = 0;

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct VirtualKeyboard {
    /// Layer used when no modifier keys are pressed.
    locked_layer: LayerIdx,

    /// Keeps track of layer activations over time.
    ///
    /// The first entry is always the base layer.
    /// The last element is the index of the currently active layer.
    layer_history: Vec<LayerIdx>,

    /// Keeps track of all pressed keys so that we always can send the matching
    /// action after key release, even when the layer has changed.
    /// Chronological order is crucial to be able to find the active layer when
    /// a modifier or lock key is pressed or released.
    pressed_keys: Vec<(ScanCode, Option<KeyAction>)>,

    /// Immutable information about the layout.
    layout: Layout,
}

impl VirtualKeyboard {
    /// Create a new virtual keyboard with `layout`.
    pub fn new(layout: Layout) -> Self {
        Self {
            locked_layer: BASE_LAYER,
            layer_history: vec![BASE_LAYER],
            pressed_keys: Vec::new(),
            layout,
        }
    }

    pub fn reset(&mut self) {
        self.locked_layer = BASE_LAYER;
        self.layer_history = vec![BASE_LAYER];
        self.pressed_keys.clear();
    }

    fn active_layer_idx(&self) -> LayerIdx {
        *self.layer_history.last().unwrap()
    }

    #[allow(unused)]
    pub fn active_layer(&self) -> &str {
        self.layout.layer_name(self.active_layer_idx())
    }

    #[allow(unused)]
    pub fn locked_layer(&self) -> &str {
        self.layout.layer_name(self.locked_layer)
    }

    pub fn caps_lock_enabled(&self) -> bool {
        matches!(self.layout.caps_lock_layer(), Some(layer) if self.locked_layer == layer)
    }

    /// Returns the layer activated by the currently pressed modifier keys.
    fn find_layer_activation(&self, starting_layer: LayerIdx) -> LayerIdx {
        let mut layer = starting_layer;
        for (scan_code, _) in &self.pressed_keys {
            if let Some(target_layer) = self.layout.layer_modifier(layer, *scan_code) {
                layer = target_layer;
            }
        }
        layer
    }

    fn update_layer_history(&mut self) {
        let new_active_layer = self.find_layer_activation(self.locked_layer);

        // Check if the active layer is in the history already.
        // This usually happens when a modifier key is released and we go back
        // to the previous layer.
        let mut layer_idx = None;
        for idx in (0..self.layer_history.len()).rev() {
            if self.layer_history[idx] == new_active_layer {
                layer_idx = Some(idx);
                break;
            }

            if self.layer_history[idx] == self.locked_layer {
                break;
            }
        }

        // Update layer history.
        if let Some(idx) = layer_idx {
            // Remove all layers “newer” than the active layer.
            self.layer_history.drain(idx + 1..);
        } else {
            // Active layer not found, add it. This usually happens when pressing
            // a modifier.
            self.layer_history.push(new_active_layer);
        }
    }

    pub fn lock_layer(&mut self, layer: LayerIdx) {
        // Jump back in history if this layer was locked before.
        if let Some(idx) = self.layer_history.iter().position(|l| *l == layer) {
            self.layer_history.drain(idx + 1..);
        }

        self.locked_layer = layer;
    }

    fn take_pressed(&mut self, scan_code: ScanCode) -> Option<Option<KeyAction>> {
        let idx = self
            .pressed_keys
            .iter()
            .position(|(sc, _action)| *sc == scan_code)?;
        Some(self.pressed_keys.remove(idx).1)
    }

    /// Returns the key action associated with the scan code press.
    pub fn press_key(&mut self, scan_code: ScanCode) -> Option<KeyAction> {
        // Get the active action if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        if let Some(action) = self.take_pressed(scan_code) {
            // Re-insert to correct history of pressed modifiers in case we we
            // missed a modifier release event.
            self.pressed_keys.push((scan_code, action));
            return action;
        }

        // Get the key action from the current layer. If the key is not available on
        // the current layer, check the previous layer. Repeat until a action was
        // found or we run out of layers.
        let action = self
            .layer_history
            .iter()
            .rev()
            .find_map(|layer| self.layout.action(*layer, scan_code));

        if self.locked_layer == BASE_LAYER {
            if let Some(target_layer) = self.layout.layer_lock(self.active_layer_idx(), scan_code) {
                self.lock_layer(target_layer);
            }
        } else {
            // Try to unlock a previously locked layer
            let active_layer_from_base = self.find_layer_activation(BASE_LAYER);
            let layer_to_lock = self.layout.layer_lock(active_layer_from_base, scan_code);
            if layer_to_lock == Some(self.locked_layer) {
                self.lock_layer(BASE_LAYER);
            }
        }

        self.pressed_keys.push((scan_code, action));
        self.update_layer_history();

        action
    }

    /// Returns the key action associated with the scan code release.
    pub fn release_key(&mut self, scan_code: ScanCode) -> Option<KeyAction> {
        // Release the pressed key.
        // If not found in the set of pressed keys forward the release action.
        // Forwarding instead of ignoring is important in following scenario:
        // 1. hook running in a user mode process
        // 2. an elevated window receives a key down event (which our hook does
        //    not get and hence is not in our set of pressed keys) e.g. alt
        // 3. switching to a non-elevated window (e.g. alt+tab)
        // 4. release of key key e.g. alt
        // --> If we don ot forward the key release here the alt key is stuck
        //     until it is pressed again.
        let presed_key = self.take_pressed(scan_code).flatten();

        self.update_layer_history();

        presed_key
    }
}

#[cfg(test)]
mod tests {
    use crate::layout::{KeyAction::*, LayoutBuilder};

    use super::*;

    #[test]
    fn layer_activation() {
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let a = layout.add_layer(String::from("a"));
        let b = layout.add_layer(String::from("b"));
        let c = layout.add_layer(String::from("c"));
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

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let shift = layout.add_layer(String::from("shift"));
        layout.add_modifier(0x2A, base, shift);
        layout.add_key(0x2A, base, VirtualKey(0xA0));
        layout.add_modifier(0xE036, base, shift);
        layout.add_key(0xE036, base, VirtualKey(0xA1));

        let mut kb = VirtualKeyboard::new(layout.build());

        assert_eq!(kb.press_key(0xE036), Some(VirtualKey(0xA1)));
        assert_eq!(kb.press_key(0x002A), Some(VirtualKey(0xA0)));
        assert_eq!(kb.release_key(0x002A), Some(VirtualKey(0xA0)));
        assert_eq!(kb.release_key(0xE036), Some(VirtualKey(0xA1)));
    }

    #[test]
    fn masked_modifier_on_base_layer() {
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let a = layout.add_layer(String::from("a"));
        let b = layout.add_layer(String::from("b"));
        let c = layout.add_layer(String::from("c"));
        layout.add_modifier(0x0A, base, a);
        layout.add_key(0x0A, base, Ignore);
        layout.add_modifier(0x0B, base, b);
        layout.add_key(0x0B, base, Ignore);
        layout.add_modifier(0x0C, a, c);
        layout.add_key(0x0C, a, Ignore);
        layout.add_key(0xBB, b, Character('B'));
        layout.add_key(0xCC, c, Character('C')); // not reachable from base

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let a = layout.add_layer(String::from("a"));
        let b = layout.add_layer(String::from("b"));
        let c = layout.add_layer(String::from("c"));

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

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let a = layout.add_layer(String::from("a"));
        let b = layout.add_layer(String::from("b"));
        let c = layout.add_layer(String::from("c"));

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

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let a = layout.add_layer(String::from("a"));
        let b = layout.add_layer(String::from("b"));
        let c = layout.add_layer(String::from("c"));
        let d = layout.add_layer(String::from("d"));

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

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let shift = layout.add_layer(String::from("shift"));

        // base layer
        layout.add_layer_lock(0x3A, base, shift); // caps lock
        layout.add_key(0x3A, base, VirtualKey(0x14)); // forward caps vk
        layout.add_key(0xFF, base, Character('x'));

        // shift layer
        layout.add_key(0xFF, shift, Character('X'));

        let mut kb = VirtualKeyboard::new(layout.build());

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
        let mut layout = LayoutBuilder::new();
        let base = layout.add_layer(String::from("base"));
        let shift = layout.add_layer(String::from("shift"));

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

        let mut kb = VirtualKeyboard::new(layout.build());

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
}
