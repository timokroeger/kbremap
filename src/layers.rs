//! Remapping and layer switching logic.

use std::collections::{HashMap, HashSet};

use anyhow::{ensure, Context, Result};

use crate::config::Config;
use crate::keyboard_hook::Remap;

/// Mapping table for a virtual keyboard layer.
#[derive(Debug)]
struct Layer {
    mappings: HashMap<u16, Remap>,

    /// Sequences of modifier keys that activate this layer.
    activation_sequences: Vec<Vec<u16>>,
}

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct Layers {
    layers: Vec<Layer>,

    /// Keys used for layer switching.
    modifiers: HashSet<u16>,

    /// Currently pressed layer modifiers keys.
    pressed_modifiers: Vec<u16>,

    /// Currently pressed keys.
    pressed_keys: HashMap<u16, Remap>,
}

/// Looks for invalid references and cycles in the layer graph.
fn check_layer_graph<'a, 'b>(
    layer_name: &'a str,
    layer_graph: &'b HashMap<&str, Vec<(u16, &'a str)>>,
    visited: &'b mut HashSet<&'a str>,
    finished: &'b mut HashSet<&'a str>,
) -> Result<()> {
    let layer = layer_graph
        .get(layer_name)
        .context(format!("Invalid layer reference {:?}", layer_name))?;
    visited.insert(layer_name);
    for (scan_code, target_layer) in layer {
        ensure!(
            !visited.contains(target_layer) || finished.contains(target_layer),
            "Cycle in layer graph: scan_code={:#06X}, layer={:?}, target_layer={:?}",
            scan_code,
            layer_name,
            target_layer
        );

        check_layer_graph(target_layer, layer_graph, visited, finished)?;
    }
    finished.insert(layer_name);
    Ok(())
}

/// Traverses the graph starting at the base layer and stores path (scan code
/// of each edge) to a layer as activation sequence for that layer.
fn build_activation_sequences<'a, 'b>(
    layer: &'a str,
    layer_graph: &'b HashMap<&str, Vec<(u16, &'a str)>>,
    activation_sequences: &'b mut HashMap<&'a str, Vec<Vec<u16>>>,
) {
    let our_seqs = activation_sequences[layer].clone();
    for (scan_code, target_layer) in &layer_graph[layer] {
        let target_seqs = activation_sequences.entry(target_layer).or_default();
        for mut seq in our_seqs.iter().cloned() {
            seq.push(*scan_code);
            target_seqs.push(seq);
        }

        build_activation_sequences(target_layer, layer_graph, activation_sequences);
    }
}

impl Layers {
    pub fn new(config: &Config) -> Result<Layers> {
        // Virtual keyboard layer activation can be viewed as graph where layers
        // are nodes and layer action keys are egdes.
        let mut layer_graph: HashMap<&str, Vec<(u16, &str)>> = HashMap::new();
        for layer_name in config.layer_names() {
            layer_graph.insert(layer_name, config.layer_modifiers(layer_name).collect());
        }

        // TODO: Smart way to figure out the base layer.

        // Layer graph validation
        let mut visited = HashSet::new();
        let mut finished = HashSet::new();
        check_layer_graph("base", &layer_graph, &mut visited, &mut finished)?;
        for layer_name in config.layer_names() {
            if !finished.contains(layer_name) {
                println!("Warning: Unused layer {:?}", layer_name);
            }
        }

        let mut activation_sequences = HashMap::new();
        activation_sequences.insert("base", vec![Vec::new()]);
        build_activation_sequences("base", &layer_graph, &mut activation_sequences);

        // Get a set of all modifiers.
        let modifiers = activation_sequences
            .iter()
            .map(|(_, seqs)| seqs.iter().map(|seq| seq.iter()).flatten().copied())
            .flatten()
            .collect();

        let mut layers = Vec::new();
        for (layer_name, activation_sequences) in activation_sequences {
            layers.push(Layer {
                mappings: config.layer_mappings(layer_name),
                activation_sequences,
            });
        }

        Ok(Layers {
            layers,
            modifiers,
            pressed_modifiers: Vec::new(),
            pressed_keys: HashMap::new(),
        })
    }

    /// Returns the currently active layer or `None` when no layer is active.
    ///
    /// A layer is considered to be active when an chronologically ordered set
    /// of pressed modifer keys matches the layer's activation sequence. This
    /// is true even when modifier keys are removed from the set randomly.
    fn active_layer(&self) -> Option<&Layer> {
        for layer in &self.layers {
            if layer.activation_sequences.contains(&self.pressed_modifiers) {
                return Some(layer);
            }
        }

        None
    }

    fn get_remapping_current_layer(&mut self, scan_code: u16) -> Option<Remap> {
        match self.active_layer() {
            Some(layer) => layer.mappings.get(&scan_code).copied(),
            None => Some(Remap::Ignore),
        }
    }

    /// Processes modifers to update select the correct layer.
    fn process_modifiers(&mut self, scan_code: u16, up: bool) {
        if !self.modifiers.contains(&scan_code) {
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
            _ => {} // Ignore repeated key presses
        }
    }

    /// Returs the remap action associated with the scan code.
    pub fn get_remapping(&mut self, scan_code: u16, up: bool) -> Option<Remap> {
        // Get the active remapping if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        // When the key is not pressed, get the mapping from the current layer.
        let remap = self
            .pressed_keys
            .remove(&scan_code)
            .or_else(|| self.get_remapping_current_layer(scan_code));

        self.process_modifiers(scan_code, up);

        if !up {
            if let Some(remap) = remap {
                self.pressed_keys.insert(scan_code, remap);
            }
        }

        remap
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

        // L2 -> XX -> L1
        assert_eq!(layers.get_remapping(0x12, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, false), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x20, true), Some(Character('2')));
        assert_eq!(layers.get_remapping(0x11, false), None);
        assert_eq!(layers.get_remapping(0x20, false), Some(Ignore));
        assert_eq!(layers.get_remapping(0x20, true), Some(Ignore));
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
}
