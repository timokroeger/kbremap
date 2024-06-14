//! Remapping and layer switching logic.

use std::collections::HashMap;

use petgraph::algo;
use petgraph::visit::EdgeRef;

use crate::layout::{KeyAction, Layout};
use crate::{LayerGraph, LayerIdx, ScanCode};

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct VirtualKeyboard<'l> {
    /// Active layer graph to figure out which layer is active.
    active_layer_graph: LayerGraph,

    /// Layer used when no modifier keys are pressed.
    locked_layer: LayerIdx,

    /// Keeps track of layer activations over time.
    ///
    /// The first entry is always the base layer.
    /// The last element is the index of the currently active layer.
    layer_history: Vec<LayerIdx>,

    /// Chronologically sorted modifier scan codes used to traverse the
    /// layer graph to determine the active layer.
    pressed_modifiers: Vec<ScanCode>,

    /// Keeps track of all pressed keys so that we always can send the matching
    /// action after key release, even when the layer has changed.
    pressed_keys: HashMap<ScanCode, Option<KeyAction>>,

    /// Immutable information about the layout. Used to re-build the active
    /// layer graph when a new layer is locked.
    layout: &'l Layout,
}

impl<'l> VirtualKeyboard<'l> {
    /// Create a new virtual keyboard with `layout`.
    pub fn new(layout: &'l Layout) -> Self {
        Self {
            active_layer_graph: layout.layer_graph.clone(),
            locked_layer: layout.base_layer,
            layer_history: vec![layout.base_layer],
            pressed_modifiers: Vec::new(),
            pressed_keys: HashMap::new(),
            layout,
        }
    }

    fn active_layer_idx(&self) -> LayerIdx {
        *self.layer_history.last().unwrap()
    }

    pub fn active_layer(&self) -> &str {
        &self.layout.layer_graph[self.active_layer_idx()]
    }

    pub fn locked_layer(&self) -> &str {
        &self.layout.layer_graph[self.locked_layer]
    }

    /// Returns the layer activated by the currently pressed modifier keys.
    fn find_layer_activation(&self, graph: &LayerGraph, starting_layer: LayerIdx) -> LayerIdx {
        let mut layer = starting_layer;
        for modifier_scan_code in &self.pressed_modifiers {
            if let Some(edge) = graph
                .edges(layer)
                .find(|edge| edge.weight().contains(modifier_scan_code))
            {
                layer = edge.target();
            }
        }
        layer
    }

    fn update_layer_history(&mut self) {
        let new_active_layer =
            self.find_layer_activation(&self.active_layer_graph, self.locked_layer);

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
        self.active_layer_graph.clone_from(&self.layout.layer_graph);

        // Update graph with the locked layer as new base layer.
        reverse_edges(&mut self.active_layer_graph, self.layout.base_layer, layer);

        // Jump back in history if this layer was locked before.
        if let Some(idx) = self.layer_history.iter().position(|l| *l == layer) {
            self.layer_history.drain(idx + 1..);
        }

        self.locked_layer = layer;
        self.update_layer_history();
    }

    fn press_modifier(&mut self, scan_code: ScanCode) {
        if self.pressed_modifiers.last() == Some(&scan_code) {
            // Ignore repeated key presses
            return;
        }

        // In case we missed a modifier release event remove the previous entry
        // to correct the history of modifier key presses.
        self.pressed_modifiers
            .retain(|pressed_scan_code| *pressed_scan_code != scan_code);
        self.pressed_modifiers.push(scan_code);
        self.update_layer_history();
    }

    /// Returns the key action associated with the scan code press.
    pub fn press_key(&mut self, scan_code: ScanCode) -> Option<KeyAction> {
        // Get the active action if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        let action = self.pressed_keys.remove(&scan_code).unwrap_or_else(|| {
            // Get the key action from the current layer. If the key is not available on
            // the current layer, check the previous layer. Repeat until a action was
            // found or we run out of layers.
            self.layer_history
                .iter()
                .rev()
                .find_map(|layer| self.layout.keymap.get(&(*layer, scan_code)).copied())
        });

        if self.layout.modifier_scan_codes.contains(&scan_code) {
            self.press_modifier(scan_code);
        }
        self.pressed_keys.insert(scan_code, action);

        action
    }

    /// Returns the key action associated with the scan code release.
    pub fn release_key(&mut self, scan_code: ScanCode) -> Option<KeyAction> {
        // Release from pressed modifiers if it was one.
        if let Some(idx) = self
            .pressed_modifiers
            .iter()
            .rposition(|pressed_scan_code| *pressed_scan_code == scan_code)
        {
            self.pressed_modifiers.remove(idx);
            self.update_layer_history();

            let layout = &self.layout;

            // Update layer locks on release. If we changed the lock state on press,
            // a repeated key event would unlock the layer again right away.
            if let Some(target_layer) = layout.locks.get(&(self.active_layer_idx(), scan_code)) {
                self.lock_layer(*target_layer);
            } else {
                // Try to unlock a previously locked layer
                let active_layer_from_base =
                    self.find_layer_activation(&layout.layer_graph, layout.base_layer);
                let layer_to_lock = layout.locks.get(&(active_layer_from_base, scan_code));
                if layer_to_lock == Some(&(self.locked_layer)) {
                    self.lock_layer(layout.base_layer);
                }
            }
        }

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
        self.pressed_keys.remove(&scan_code).flatten()
    }
}

/// Reverses the direction of edges on all paths between node `from` and `to`.
fn reverse_edges(graph: &mut LayerGraph, from: LayerIdx, to: LayerIdx) {
    let paths: Vec<_> = algo::all_simple_paths::<Vec<_>, _>(&*graph, from, to, 0, None).collect();
    let edges: Vec<[LayerIdx; 2]> = paths
        .iter()
        .flat_map(|path| path.windows(2))
        .map(|edge| edge.try_into().unwrap())
        .collect();

    // Reverse the edge
    for [from, to] in edges {
        if let Some(edge) = graph.find_edge(from, to) {
            let scan_code = graph.remove_edge(edge).unwrap();
            graph.add_edge(to, from, scan_code);
        }
    }
}
