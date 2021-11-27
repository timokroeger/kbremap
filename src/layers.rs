//! Remapping and layer switching logic.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{algo, Directed, Graph};

use crate::config::Config;
use crate::keyboard_hook::Remap;

/// An iterator over layers activated by pressed modifiers.
///
/// This struct is created by [`Layers::layer_activations`].
struct LayerActivations<'a> {
    layers: &'a Layers,
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
                .find(|edge| *edge.weight() == layers.pressed_modifiers[i])
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

/// Mapping table for a virtual keyboard layer.
#[derive(Debug)]
struct Layer {
    name: String,
    mappings: HashMap<u16, Remap>,
}

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct Layers {
    /// A keyboard layout can be viewed as graph where layers are the nodes and
    /// modifiers (layer change keys) are the egdes.
    layer_graph: Graph<Layer, u16, Directed, u8>,

    /// Set of unique scan codes used for layer switching.
    modifiers_scan_codes: Vec<u16>,

    base_layer: NodeIndex<u8>,
    locked_layer: NodeIndex<u8>,
    active_layer: NodeIndex<u8>,

    pressed_keys: HashMap<u16, Option<Remap>>,
    pressed_modifiers: Vec<u16>,
}

impl Layers {
    /// Constructs the layers from a configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let mut layer_graph = Graph::default();

        for layer in config.layer_names() {
            layer_graph.add_node(Layer {
                name: String::from(layer),
                mappings: config.layer_mappings(layer),
            });
        }

        let mut modifiers_scan_codes = Vec::new();
        for from in layer_graph.node_indices() {
            for (scan_code, target_layer) in config.layer_modifiers(&layer_graph[from].name) {
                for to in layer_graph.node_indices() {
                    if layer_graph[to].name == target_layer {
                        layer_graph.add_edge(from, to, scan_code);
                        modifiers_scan_codes.push(scan_code);
                    }
                }
            }
        }

        let base_layer =
            algo::toposort(&layer_graph, None).map_err(|_| anyhow!("Cycle in layer graph"))?[0];

        // Get a set of all unique modifiers.
        modifiers_scan_codes.dedup();

        Ok(Self {
            layer_graph,
            modifiers_scan_codes,
            base_layer,
            locked_layer: base_layer,
            active_layer: base_layer,
            pressed_keys: HashMap::new(),
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

    /// Checks if the key is a modifier and updates the active layer accordingly.
    fn update_modifiers(&mut self, scan_code: u16, up: bool) {
        if !self.modifiers_scan_codes.contains(&scan_code) {
            return;
        }

        let active_idx = self
            .pressed_modifiers
            .iter()
            .rposition(|&pressed_scan_code| pressed_scan_code == scan_code);
        match (active_idx, up) {
            (None, false) => {
                self.pressed_modifiers.push(scan_code);
            }
            (Some(idx), true) => {
                self.pressed_modifiers.remove(idx);
            }
            _ => return, // Ignore repeated key presses
        }

        let mut layer_activations = self.layer_activations();
        self.active_layer = if let Some(active_layer) = layer_activations.next() {
            // Lock the layer if we find a second sequence for this layer
            // Example: Both shift key pressed to lock the shift layer (caps lock functionality).
            if layer_activations.any(|layer| layer == active_layer) {
                // Restore original graph when a layer was locked already.
                reverse_edges(&mut self.layer_graph, self.locked_layer, self.base_layer);

                // Update graph with the locked layer as new base layer.
                reverse_edges(&mut self.layer_graph, self.base_layer, active_layer);

                self.locked_layer = active_layer;
            }

            active_layer
        } else {
            self.locked_layer
        }
    }

    /// Returs the remap action associated with the scan code.
    pub fn get_remapping(&mut self, scan_code: u16, up: bool) -> Option<Remap> {
        // Get the active remapping if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        let remap = self.pressed_keys.remove(&scan_code).unwrap_or_else(|| {
            self.layer_graph[self.active_layer]
                .mappings
                .get(&scan_code)
                .copied()
        });

        self.update_modifiers(scan_code, up);

        if !up {
            self.pressed_keys.insert(scan_code, remap);
        }

        remap
    }
}

/// Reverses the direction of edges on all paths between node `from` and `to`.
fn reverse_edges(
    graph: &mut Graph<Layer, u16, Directed, u8>,
    from: NodeIndex<u8>,
    to: NodeIndex<u8>,
) {
    let paths: Vec<_> = algo::all_simple_paths::<Vec<_>, _>(&*graph, from, to, 0, None).collect();
    let mut edges: Vec<[NodeIndex<u8>; 2]> = paths
        .iter()
        .flat_map(|path| path.windows(2))
        .map(|edge| edge.try_into().unwrap())
        .collect();

    // Keep unique edges only.
    // `algo::all_simple_paths` returns a lot of duplicate paths when multiple
    // edges (layer keys) exists between a node (layer). There may also be duplicates
    // when paths are overlapping.
    // TODO: combine edges when building the layer graph?
    edges.dedup();

    // Reverse the edge
    for [from, to] in edges {
        while let Some(edge) = graph.find_edge(from, to) {
            let scan_code = graph.remove_edge(edge).unwrap();
            graph.add_edge(to, from, scan_code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_activation() -> anyhow::Result<()> {
        let config_str = r#"[layers]
        base = [
            { scan_code = 0x11, layer = "l1" },
            { scan_code = 0x12, layer = "l2" },
            { scan_code = 0x20, characters = "0" },
        ]
        l1 = [{ scan_code = 0x12, layer = "l3" }, { scan_code = 0x20, characters = "1" }]
        l2 = [{ scan_code = 0x20, characters = "2" }]
        l3 = [{ scan_code = 0x20, characters = "3" }]
        "#;

        let config = Config::from_toml(config_str)?;
        let mut layers = Layers::new(&config)?;

        use Remap::*;

        // L0
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        // L1
        assert_eq!(layers.get_remapping(0x11, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x11, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        // L2
        assert_eq!(layers.get_remapping(0x12, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x12, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        // L1 -> L3 -> L2
        assert_eq!(layers.get_remapping(0x11, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x12, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('3')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('3')));
        assert_eq!(layers.get_remapping(0x11, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x12, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        // L2 -> XX (L2 still active) -> L1
        assert_eq!(layers.get_remapping(0x12, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x11, false), None);
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x12, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x11, true), None);
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        // Change layer during key press
        assert_eq!(layers.get_remapping(0x11, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x11, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('1')));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('0')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('0')));

        Ok(())
    }

    #[test]
    fn accidental_shift_lock_issue25() -> anyhow::Result<()> {
        let config_str = r#"[layers]
        base = [
            { scan_code = 0x2A, layer = "shift", virtual_key = 0xA0 }, # left shift
            { scan_code = 0xE036, layer = "shift", virtual_key = 0xA1 }, # right shift
        ]
        shift = []
        "#;

        let config = Config::from_toml(config_str)?;
        let mut layers = Layers::new(&config)?;

        use Remap::*;

        assert_eq!(layers.get_remapping(0xE036, false), Some(VirtualKey(0xA1)));
        assert_eq!(layers.get_remapping(0x002A, false), None);
        assert_eq!(layers.get_remapping(0x002A, true), None);
        assert_eq!(layers.get_remapping(0xE036, true), Some(VirtualKey(0xA1)));

        Ok(())
    }

    #[test]
    fn cyclic_layers() {
        let config_str = r#"[layers]
        base = [{ scan_code = 0x0001, layer = "overlay" }]
        overlay = [{ scan_code = 0x0002, layer = "base" }]
        "#;

        let config = Config::from_toml(config_str).unwrap();
        assert!(Layers::new(&config).is_err());
    }

    #[test]
    fn masked_modifier_on_base_layer() -> anyhow::Result<()> {
        let config_str = r#"[layers]
        base = [{ scan_code = 0x0A, layer = "a" }, { scan_code = 0x0B, layer = "b" }]
        a = [{ scan_code = 0x0C, layer = "c" }]
        b = [{ scan_code = 0xBB, characters = "B" }]
        c = [{ scan_code = 0xCC, characters = "C" }] # not reachable from base
        "#;

        let config = Config::from_toml(config_str)?;
        let mut layers = Layers::new(&config)?;

        use Remap::*;

        // "B" does not exist on base layer
        assert_eq!(layers.get_remapping(0xBB, false), None);
        assert_eq!(layers.get_remapping(0xBB, true), None);

        // Layer c should not be activated from the base layer
        assert_eq!(layers.get_remapping(0x0C, false), None);
        assert_eq!(layers.get_remapping(0xCC, false), None);
        assert_eq!(layers.get_remapping(0xCC, true), None);

        // But Layer b should be activated even when modifier for layer c pressed.
        assert_eq!(layers.get_remapping(0x0B, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0xBB, false), Some(Character('B')));
        assert_eq!(layers.get_remapping(0xBB, true), Some(Character('B')));

        // Release layer c key (it was never activated) and make sure we are still on layer b.
        assert_eq!(layers.get_remapping(0x0C, true), None);
        assert_eq!(layers.get_remapping(0xBB, false), Some(Character('B')));
        assert_eq!(layers.get_remapping(0xBB, true), Some(Character('B')));

        // Release leayer b key
        assert_eq!(layers.get_remapping(0x0B, true), Some(Ignore));

        // "B" does not exist on base layer
        assert_eq!(layers.get_remapping(0xBB, false), None);
        assert_eq!(layers.get_remapping(0xBB, true), None);

        Ok(())
    }

    #[test]
    fn layer_lock() -> anyhow::Result<()> {
        let config_str = r#"[layers]
        base = [
            { scan_code = 0x0A, layer = "a" },
            { scan_code = 0xA0, layer = "a" },
            { scan_code = 0x0B, layer = "b" },
            { scan_code = 0xB0, layer = "b" },
        ]
        a = [
            { scan_code = 0x0B, layer = "c" },
            { scan_code = 0xB0, layer = "c" },
            { scan_code = 0xAA, characters = "A" },
        ]
        b = [
            { scan_code = 0x0A, layer = "c" },
            { scan_code = 0xA0, layer = "c" },
            { scan_code = 0xBB, characters = "B" },
        ]
        c = [{ scan_code = 0xCC, characters = "C" }]
        "#;

        let config = Config::from_toml(config_str)?;
        let mut layers = Layers::new(&config)?;

        use Remap::*;

        // Lock layer a
        assert_eq!(layers.get_remapping(0x0A, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0xA0, false), None);
        assert_eq!(layers.get_remapping(0x0A, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0xA0, true), None);

        // Test if locked
        assert_eq!(layers.get_remapping(0xAA, false), Some(Character('A')));
        assert_eq!(layers.get_remapping(0xAA, true), Some(Character('A')));

        // Temp switch back to layer base
        assert_eq!(layers.get_remapping(0x0A, false), None);
        assert_eq!(layers.get_remapping(0xAA, false), None);
        assert_eq!(layers.get_remapping(0xAA, true), None);
        assert_eq!(layers.get_remapping(0x0A, true), None);

        // Temp switch to layer c
        assert_eq!(layers.get_remapping(0x0B, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0xCC, false), Some(Character('C')));
        assert_eq!(layers.get_remapping(0xCC, true), Some(Character('C')));

        // Lock layer c
        assert_eq!(layers.get_remapping(0xB0, false), None);
        assert_eq!(layers.get_remapping(0xB0, true), None);

        // Temp switched to layer a still
        assert_eq!(layers.get_remapping(0xAA, false), Some(Character('A')));
        assert_eq!(layers.get_remapping(0xAA, true), Some(Character('A')));

        // Check if locked to layer c
        assert_eq!(layers.get_remapping(0x0B, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0xCC, false), Some(Character('C')));
        assert_eq!(layers.get_remapping(0xCC, true), Some(Character('C')));

        // Lock layer base again
        assert_eq!(layers.get_remapping(0xB0, false), None);
        assert_eq!(layers.get_remapping(0xA0, false), None);
        assert_eq!(layers.get_remapping(0x0A, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x0B, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0xB0, true), None);
        assert_eq!(layers.get_remapping(0xA0, true), None);
        assert_eq!(layers.get_remapping(0x0A, true), Some(Ignore));
        assert_eq!(layers.get_remapping(0x0B, true), Some(Ignore));

        // Check if locked to layer base
        assert_eq!(layers.get_remapping(0xAA, false), None);
        assert_eq!(layers.get_remapping(0xAA, true), None);

        Ok(())
    }
}
