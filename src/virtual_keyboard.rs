//! Remapping and layer switching logic.

use crate::layout::{KeyAction, Layout};
use crate::{LayerIdx, ScanCode};

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

    pub fn locked_layer_idx(&self) -> LayerIdx {
        self.locked_layer
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
