//! Remapping and layer switching logic.

use map_vec::{Map, Set};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{algo, Directed, Graph};

use crate::layout::{KeyAction, Layout};

/// A keyboard layout can be viewed as graph where layers are the nodes and
/// modifiers (layer change keys) are the egdes.
type LayerGraph = Graph<(), Vec<u16>, Directed, u8>;

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct VirtualKeyboard {
    /// Describes the key mappings of this virtual keyboard.
    layout: Layout,

    /// Set of unique scan codes used for layer switching.
    modifiers_scan_codes: Set<u16>,

    /// Immutable graph used as reference to rebuild the active layer graph when
    /// the locked layer changes.
    base_layer_graph: LayerGraph,
    base_layer: NodeIndex<u8>,

    /// Active layer graph to figure out which layer is active.
    active_layer_graph: LayerGraph,

    /// Layer used when no modifier keys are pressed.
    locked_layer: NodeIndex<u8>,

    /// Keeps track of layer activations over time.
    ///
    /// The first entry is always the base layer.
    /// The last element is the index of the currently active layer.
    layer_history: Vec<NodeIndex<u8>>,

    /// Chronologically sorted modifier scan codes used to traverse the
    /// layer graph to determine the active layer.
    pressed_modifiers: Vec<u16>,

    /// Keeps track of all pressed keys so that we always can send the matching
    /// action after key release, even when the layer has changed.
    pressed_keys: Map<u16, Option<KeyAction>>,
}

impl VirtualKeyboard {
    /// Create a new virtual keyboard with `layout`.
    pub fn new(layout: Layout) -> Self {
        // Add modifiers as edges to the graph.
        // Include also lock modifiers so that they change to the target layer on key press
        // (even before the locking locking runs on key release).

        // Collect and sort modifiers to make merging easy.
        let mut modifiers: Vec<_> = layout.modifiers().chain(layout.layer_locks()).collect();
        modifiers.sort_by(|a, b| {
            a.layer_from
                .cmp(&b.layer_from)
                .then(a.layer_to.cmp(&b.layer_to))
        });

        let mut modifiers_scan_codes = Set::new();
        let mut edges: Vec<(u8, u8, Vec<u16>)> = Vec::new();
        for modifier in modifiers {
            modifiers_scan_codes.insert(modifier.scan_code);

            // A lock modifier key can lock the layer it is defined on by targeting the own layer.
            // Skip adding those self-lock modifier to the layer graph to prevent cycles.
            // They would not have any effect because they do not change the layer.
            if modifier.layer_from == modifier.layer_to {
                continue;
            }

            // Merge multiple modifiers between two layers into a single edge to make partial graph
            // reversal for layer locking easier.
            match edges.last_mut() {
                Some(last) if last.0 == modifier.layer_from && last.1 == modifier.layer_to => {
                    last.2.push(modifier.scan_code)
                }
                _ => edges.push((
                    modifier.layer_from,
                    modifier.layer_to,
                    vec![modifier.scan_code],
                )),
            }
        }

        let layer_graph: LayerGraph = Graph::from_edges(edges);

        // Check for cycles and find the base layer.
        let base_layer = algo::toposort(&layer_graph, None).expect("Cycle in layer graph")[0];

        Self {
            layout,
            modifiers_scan_codes,
            base_layer_graph: layer_graph.clone(),
            base_layer,
            active_layer_graph: layer_graph,
            locked_layer: base_layer,
            layer_history: vec![base_layer],
            pressed_modifiers: Vec::new(),
            pressed_keys: Map::new(),
        }
    }

    pub fn active_layer(&self) -> u8 {
        self.layer_history[self.layer_history.len() - 1].index() as u8
    }

    pub fn locked_layer(&self) -> u8 {
        self.locked_layer.index() as u8
    }

    /// Returns the layer activated by the currently pressed modifier keys.
    fn find_layer_activation(
        &self,
        graph: &LayerGraph,
        starting_layer: NodeIndex<u8>,
    ) -> NodeIndex<u8> {
        let mut layer = starting_layer;
        for i in 0..self.pressed_modifiers.len() {
            if let Some(edge) = graph
                .edges(layer)
                .find(|edge| edge.weight().contains(&self.pressed_modifiers[i]))
            {
                layer = edge.target();
            } else {
                continue;
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

    pub fn lock_layer(&mut self, layer: u8) {
        let layer = layer.into();

        self.active_layer_graph.clone_from(&self.base_layer_graph);

        // Update graph with the locked layer as new base layer.
        reverse_edges(&mut self.active_layer_graph, self.base_layer, layer);

        // Jump back in history if this layer was locked before.
        if let Some(idx) = self.layer_history.iter().position(|l| *l == layer) {
            self.layer_history.drain(idx + 1..);
        }

        self.locked_layer = layer;
        self.update_layer_history();
    }

    fn press_modifier(&mut self, scan_code: u16) {
        if self.pressed_modifiers.last() == Some(&scan_code) {
            // Ignore repeated key presses
            return;
        }

        if let Some(idx) = self
            .pressed_modifiers
            .iter()
            .position(|pressed_scan_code| *pressed_scan_code == scan_code)
        {
            // We must have missed a key release event.
            // Remove the previously pressed entry.
            self.pressed_modifiers.remove(idx);
        }

        self.pressed_modifiers.push(scan_code);
        self.update_layer_history();
    }

    /// Returs the key action associated with the scan code press.
    pub fn press_key(&mut self, scan_code: u16) -> Option<KeyAction> {
        // Get the active action if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        let action = self.pressed_keys.remove(&scan_code).unwrap_or_else(|| {
            let key = self.layout.get_key(scan_code);

            // Get the key action from the current layer. If the key is not available on
            // the current layer, check the previous layer. Repeat until a action was
            // found or we run out of layers.
            self.layer_history
                .iter()
                .rev()
                .find_map(|layer| key.action(layer.index() as _))
        });

        if self.modifiers_scan_codes.contains(&scan_code) {
            self.press_modifier(scan_code);
        }
        self.pressed_keys.insert(scan_code, action);

        action
    }

    /// Returs the key action associated with the scan code release.
    pub fn release_key(&mut self, scan_code: u16) -> Option<KeyAction> {
        // Release from pressed modifiers if it was one.
        if let Some(idx) = self
            .pressed_modifiers
            .iter()
            .rposition(|pressed_scan_code| *pressed_scan_code == scan_code)
        {
            self.pressed_modifiers.remove(idx);
            self.update_layer_history();

            // Update layer locks on release. If we changed the lock state on press,
            // a repeated key event would unlock the layer again right away.
            let key = self.layout.get_key(scan_code);
            if let Some(lock_layer) = key.layer_lock(self.active_layer()) {
                self.lock_layer(lock_layer);
            } else if self.locked_layer != self.base_layer {
                // Try to unlock a previously locked layer
                let active_layer_from_base =
                    self.find_layer_activation(&self.base_layer_graph, self.base_layer);
                if key.layer_lock(active_layer_from_base.index() as u8)
                    == Some(self.locked_layer.index() as u8)
                {
                    self.lock_layer(self.base_layer.index() as u8);
                }
            }
        }

        // Release the pressed key, or ignore it when the key was released without
        // it being pressed before.
        self.pressed_keys
            .remove(&scan_code)
            .unwrap_or(Some(KeyAction::Ignore))
    }
}

/// Reverses the direction of edges on all paths between node `from` and `to`.
fn reverse_edges(graph: &mut LayerGraph, from: NodeIndex<u8>, to: NodeIndex<u8>) {
    let paths: Vec<_> = algo::all_simple_paths::<Vec<_>, _>(&*graph, from, to, 0, None).collect();
    let edges: Vec<[NodeIndex<u8>; 2]> = paths
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
