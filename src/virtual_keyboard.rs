//! Remapping and layer switching logic.

use anyhow::{anyhow, Result};
use map_vec::{Map, Set};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{algo, Directed, Graph};

use crate::keyboard_hook::KeyAction;
use crate::layout::Layout;

/// A keyboard layout can be viewed as graph where layers are the nodes and
/// modifiers (layer change keys) are the egdes.
type LayerGraph = Graph<(), Vec<u16>, Directed, u8>;

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct VirtualKeyboard {
    layout: Layout,

    layer_graph_base: LayerGraph,
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

        for lock_modifiers in layout.layer_locks() {
            modifiers_scan_codes.insert(lock_modifiers.scan_code);
        }

        let layer_graph: LayerGraph = Graph::from_edges(edges);
        let base_layer =
            algo::toposort(&layer_graph, None).map_err(|_| anyhow!("Cycle in layer graph"))?[0];

        Ok(Self {
            layout,
            layer_graph_base: layer_graph.clone(),
            layer_graph,
            modifiers_scan_codes,
            base_layer,
            locked_layer: base_layer,
            layer_history: vec![base_layer],
            pressed_keys: Map::new(),
            pressed_modifiers: Vec::new(),
        })
    }

    fn active_layer(&self) -> NodeIndex<u8> {
        self.layer_history[self.layer_history.len() - 1]
    }

    /// Returns the layer activated by the currently pressed modifier keys.
    fn find_layer_activation(
        &self,
        graph: &LayerGraph,
        base_layer: NodeIndex<u8>,
    ) -> NodeIndex<u8> {
        let mut layer = base_layer;
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

    fn update_active_layer(&mut self) {
        let new_active_layer = self.find_layer_activation(&self.layer_graph, self.locked_layer);

        // When the active layer has changed search for it in the history but stop at the locked layer.
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
            self.layer_history.push(new_active_layer);
        }
    }

    pub fn lock_layer(&mut self, lock_layer: NodeIndex<u8>) {
        self.layer_graph.clone_from(&self.layer_graph_base);

        // Update graph with the locked layer as new base layer.
        reverse_edges(&mut self.layer_graph, self.base_layer, lock_layer);

        // Jump back in history if this layer was locked before.
        if let Some(idx) = self
            .layer_history
            .iter()
            .position(|layer| *layer == lock_layer)
        {
            self.layer_history.drain(idx + 1..);
        }

        self.locked_layer = lock_layer;
        self.update_active_layer();
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
            self.update_active_layer();

            // Update layer locks on release. If we changed the lock state on press,
            // a repeated key event would unlock the layer again right away.
            let key = self.layout.get_key(scan_code);
            if let Some(lock_layer) = key.layer_lock(self.active_layer().index() as u8) {
                self.lock_layer(NodeIndex::new(lock_layer.into()));
            } else if self.locked_layer != self.base_layer {
                // Try to unlock a previously locked layer
                let active_layer_from_base =
                    self.find_layer_activation(&self.layer_graph_base, self.base_layer);
                if key.layer_lock(active_layer_from_base.index() as u8)
                    == Some(self.locked_layer.index() as u8)
                {
                    self.lock_layer(self.base_layer);
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
            .add_key(0xFF, "base",Character('x'))
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
        assert_eq!(kb.release_key(0x2A), Some(VirtualKey(0xA0)));

        // temp base layer
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
}
