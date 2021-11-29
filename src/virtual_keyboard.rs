//! Remapping and layer switching logic.

use anyhow::{anyhow, Result};
use map_vec::{Map, Set};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{algo, Directed, Graph};

use crate::keyboard_hook::KeyAction;
use crate::layout::Layout;

/// An iterator over layers activated by pressed modifiers.
///
/// This struct is created by [`Layers::layer_activations`].
struct LayerActivations<'a> {
    layers: &'a VirtualKeyboard,
    idx: usize,
}

impl<'a> Iterator for LayerActivations<'a> {
    type Item = NodeIndex<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let layers = self.layers;

        let mut layer = None;
        for i in self.idx..layers.pressed_modifiers.len() {
            if let Some(edge) = layers
                .layer_graph
                .edges(layer.unwrap_or(layers.locked_layer))
                .find(|edge| edge.weight().contains(&layers.pressed_modifiers[i]))
            {
                layer = Some(edge.target());
                self.idx = i + 1;
            } else {
                continue;
            }
        }
        layer
    }
}

/// A keyboard layout can be viewed as graph where layers are the nodes and
/// modifiers (layer change keys) are the egdes.
type LayerGraph = Graph<(), Vec<u16>, Directed, u8>;

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct VirtualKeyboard {
    layout: Layout,

    layer_graph: LayerGraph,

    /// Set of unique scan codes used for layer switching.
    modifiers_scan_codes: Set<u16>,

    base_layer: NodeIndex<u8>,
    locked_layer: NodeIndex<u8>,
    layer_history: Vec<NodeIndex<u8>>,

    pressed_keys: Map<u16, Option<KeyAction>>,
    pressed_modifiers: Vec<u16>,
}

impl VirtualKeyboard {
    /// Create a new virtual keyboard with `layout`.
    pub fn new(layout: Layout) -> Result<Self> {
        let mut modifiers: Vec<_> = layout.modifiers().collect();
        modifiers.sort_by(|a, b| {
            a.layer_from
                .cmp(&b.layer_from)
                .then(a.layer_to.cmp(&b.layer_to))
        });

        // Merge modifiers between identical layers into a single edge.
        let mut edges: Vec<(u8, u8, Vec<u16>)> = Vec::new();
        let mut modifiers_scan_codes = Set::new();

        for modifier in modifiers {
            modifiers_scan_codes.insert(modifier.scan_code);

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

        let layer_graph = Graph::from_edges(edges);
        let base_layer =
            algo::toposort(&layer_graph, None).map_err(|_| anyhow!("Cycle in layer graph"))?[0];

        Ok(Self {
            layout,
            layer_graph,
            modifiers_scan_codes,
            base_layer,
            locked_layer: base_layer,
            layer_history: vec![base_layer],
            pressed_keys: Map::new(),
            pressed_modifiers: Vec::new(),
        })
    }

    /// Creates an iterator over layers activated by the currently pressed modifier keys.
    fn layer_activations(&self) -> LayerActivations {
        LayerActivations {
            layers: self,
            idx: 0,
        }
    }

    fn update_active_layer(&mut self) {
        let mut layer_activations = self.layer_activations();
        let active_layer = if let Some(active_layer) = layer_activations.next() {
            // Lock the layer if we find a second sequence for this layer
            // Example: Both shift key pressed to lock the shift layer (caps lock functionality).
            if layer_activations.any(|layer| layer == active_layer) {
                // Restore original graph when a layer was locked already.
                reverse_edges(&mut self.layer_graph, self.locked_layer, self.base_layer);

                // Update graph with the locked layer as new base layer.
                reverse_edges(&mut self.layer_graph, self.base_layer, active_layer);

                // Jump back in history if this layer was locked before.
                if let Some(idx) = self
                    .layer_history
                    .iter()
                    .position(|layer| *layer == active_layer)
                {
                    self.layer_history.drain(idx + 1..);
                }

                self.locked_layer = active_layer;
            }

            active_layer
        } else {
            self.locked_layer
        };

        // When the active layer has changed search for it in the history but stop at the locked layer.
        let mut layer_idx = None;
        for idx in (0..self.layer_history.len()).rev() {
            if self.layer_history[idx] == active_layer {
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
            self.layer_history.push(active_layer);
        }
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
        self.update_active_layer();
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
                .find_map(|layer| key.action_on_layer(layer.index() as _))
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
            self.update_active_layer();
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
        let edge = graph.find_edge(from, to).unwrap();
        let scan_code = graph.remove_edge(edge).unwrap();
        graph.add_edge(to, from, scan_code);
    }
}

#[cfg(test)]
mod tests {
    use crate::layout::LayoutBuilder;

    use super::KeyAction::*;
    use super::*;

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
            .add_key(0xFF, "a", Character('A'))
            .add_modifier(0x0A, "b", "c", None)
            .add_modifier(0xA0, "b", "c", None)
            .add_key(0xFF, "b", Character('B'))
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

        // Lock layer base again
        assert_eq!(kb.press_key(0xB0), Some(Ignore));
        assert_eq!(kb.press_key(0xA0), Some(Ignore));
        assert_eq!(kb.press_key(0x0A), Some(Ignore));
        assert_eq!(kb.press_key(0x0B), Some(Ignore));
        assert_eq!(kb.release_key(0xB0), Some(Ignore));
        assert_eq!(kb.release_key(0xA0), Some(Ignore));
        assert_eq!(kb.release_key(0x0A), Some(Ignore));
        assert_eq!(kb.release_key(0x0B), Some(Ignore));

        // Check if locked to layer base
        assert_eq!(kb.press_key(0xFF), Some(Character('X')));
        assert_eq!(kb.release_key(0xFF), Some(Character('X')));

        Ok(())
    }

    #[test]
    fn transparency() -> anyhow::Result<()> {
        let mut layout = LayoutBuilder::new();
        layout
            .add_modifier(0x0B, "a", "b", None)
            .add_modifier(0xB0, "a", "b", None)
            .add_key(0x01, "a", Character('A'))
            .add_key(0x02, "a", Character('A'))
            .add_key(0x03, "a", Character('A'))
            .add_modifier(0x0C, "b", "c", None)
            .add_modifier(0xC0, "b", "c", None)
            .add_key(0x01, "b", Character('B'))
            .add_key(0x02, "b", Character('B'))
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

        assert_eq!(kb.press_key(0x0B), Some(Ignore));

        // Layer b
        assert_eq!(kb.press_key(0x01), Some(Character('B')));
        assert_eq!(kb.release_key(0x01), Some(Character('B')));
        assert_eq!(kb.press_key(0x02), Some(Character('B')));
        assert_eq!(kb.release_key(0x02), Some(Character('B')));
        assert_eq!(kb.press_key(0x03), Some(Character('A')));
        assert_eq!(kb.release_key(0x03), Some(Character('A')));
        assert_eq!(kb.press_key(0x04), None);
        assert_eq!(kb.release_key(0x04), None);

        assert_eq!(kb.press_key(0x0C), Some(Ignore));

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
        assert_eq!(kb.press_key(0xB0), Some(Ignore));
        assert_eq!(kb.press_key(0xC0), Some(Ignore));

        // Release all but on modifier to activate layer b
        assert_eq!(kb.release_key(0xB0), Some(Ignore));
        assert_eq!(kb.release_key(0x0B), Some(Ignore));
        assert_eq!(kb.release_key(0x0C), Some(Ignore));

        dbg!(&kb.locked_layer);
        dbg!(&kb.layer_history);

        // Layer c
        assert_eq!(kb.press_key(0x01), Some(Character('B')));
        assert_eq!(kb.release_key(0x01), Some(Character('B')));
        assert_eq!(kb.press_key(0x02), Some(Character('B')));
        assert_eq!(kb.release_key(0x02), Some(Character('B')));
        assert_eq!(kb.press_key(0x03), Some(Character('A')));
        assert_eq!(kb.release_key(0x03), Some(Character('A')));
        // Should be transparent to layer c now
        assert_eq!(kb.press_key(0x04), Some(Character('C')));
        assert_eq!(kb.release_key(0x04), Some(Character('C')));

        // Lock layer a again
        assert_eq!(kb.press_key(0xB0), Some(Ignore));
        assert_eq!(kb.press_key(0x0C), Some(Ignore));
        assert_eq!(kb.press_key(0x0B), Some(Ignore));

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
}
