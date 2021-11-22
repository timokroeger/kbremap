//! Remapping and layer switching logic.

use std::collections::HashMap;

use anyhow::{ensure, Result};

use crate::config::Config;
use crate::keyboard_hook::Remap;

/// Mapping table for a virtual keyboard layer.
#[derive(Debug)]
struct Layer {
    name: String,
    mappings: HashMap<u16, Remap>,
}

/// A modifier key changes the active layer when pressed.
#[derive(Debug)]
struct LayerModifier {
    scan_code: u16,
    from: usize,
    to: usize,
}

/// Sequence of pressed modifiers that activate a layer.
#[derive(Debug, Clone)]
struct ModifierSequence {
    target_layer: usize,
    sequence: Vec<u16>,
}

/// Collection of virtual keyboard layers and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct Layers {
    /// Nodes in the layer graph.
    layers: Vec<Layer>,

    /// Edges in the layer graph.
    modifiers: Vec<LayerModifier>,

    /// Set of unique scan codes used for layer switching.
    modifiers_scan_codes: Vec<u16>,

    /// Describes how layers are activated.
    /// Rebuilt when the "base" layer changes.
    modifier_sequences: Vec<ModifierSequence>,

    /// Currently pressed layer modifiers keys.
    pressed_modifiers: Vec<u16>,

    /// Currently pressed keys.
    pressed_keys: HashMap<u16, Option<Remap>>,
}

/// Looks for invalid references and cycles in the layer graph.
fn check_layer_graph(
    modifiers: &[LayerModifier],
    layer_idx: usize,
    visited: &mut Vec<usize>,
    finished: &mut Vec<usize>,
) -> Result<()> {
    visited.push(layer_idx);
    for modifier in modifiers
        .iter()
        .filter(|modifier| modifier.from == layer_idx)
    {
        ensure!(
            !visited.contains(&modifier.to) || finished.contains(&modifier.to),
            "Cycle in layer graph: scan_code={:#06X}",
            modifier.scan_code,
        );
        check_layer_graph(modifiers, modifier.to, visited, finished)?;
    }
    finished.push(layer_idx);
    Ok(())
}

/// Traverses the graph starting at the layer specified by `layer_idx` and
/// stores the path (scan code of each edge) to a layer as modifier sequence
/// for that layer.
fn build_modifier_sequences(
    modifiers: &[LayerModifier],
    layer_idx: usize,
    modifier_sequences: &mut Vec<ModifierSequence>,
) {
    for modifier in modifiers
        .iter()
        .filter(|modifier| modifier.from == layer_idx)
    {
        // Find the sequences to this layer.
        let mut new_seqs: Vec<ModifierSequence> = modifier_sequences
            .iter()
            .filter(|modifier_sequence| modifier_sequence.target_layer == layer_idx)
            .cloned()
            .collect();

        // Append this modifier to the existing sequences
        for modifier_seq in &mut new_seqs {
            modifier_seq.sequence.push(modifier.scan_code);
            modifier_seq.target_layer = modifier.to;
        }

        modifier_sequences.extend(new_seqs);

        build_modifier_sequences(modifiers, modifier.to, modifier_sequences);
    }
}

impl Layers {
    pub fn new(config: &Config) -> Result<Layers> {
        // Virtual keyboard layer activation can be viewed as graph where layers
        // are nodes and modifiers (layer change keys) are egdes.
        let layers = Vec::from_iter(config.layer_names().map(|layer_name| Layer {
            name: String::from(layer_name),
            mappings: config.layer_mappings(layer_name),
        }));

        let mut modifiers = Vec::new();
        for (i, layer_to) in config.layer_names().enumerate() {
            for (j, layer_from) in config.layer_names().enumerate() {
                let connecting_modifiers =
                    config
                        .layer_modifiers(layer_from)
                        .filter_map(|(scan_code, target_layer)| {
                            if target_layer == layer_to {
                                Some(scan_code)
                            } else {
                                None
                            }
                        });
                for scan_code in connecting_modifiers {
                    modifiers.push(LayerModifier {
                        scan_code,
                        from: j,
                        to: i,
                    });
                }
            }
        }

        // TODO: Smart way to figure out the base layer.
        // Build modifier sequences for layer activation
        let base_idx = layers
            .iter()
            .position(|layer| layer.name == "base")
            .expect("Layer \"base\" not found");

        // Layer graph validation
        let mut visited = Vec::new();
        let mut finished = Vec::new();
        check_layer_graph(&modifiers, base_idx, &mut visited, &mut finished)?;

        // Get a set of all modifiers.
        let mut modifiers_scan_codes =
            Vec::from_iter(modifiers.iter().map(|modifier| modifier.scan_code));
        modifiers_scan_codes.dedup();

        let mut modifier_sequences = Vec::from([ModifierSequence {
            target_layer: base_idx,
            sequence: Vec::new(),
        }]);
        build_modifier_sequences(&modifiers, base_idx, &mut modifier_sequences);
        modifier_sequences
            .sort_by_key(|modifier_sequence| std::cmp::Reverse(modifier_sequence.sequence.len()));

        Ok(Layers {
            layers,
            modifiers,
            modifiers_scan_codes,
            modifier_sequences,
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
        for modifier_sequence in &self.modifier_sequences {
            let seq = &modifier_sequence.sequence;
            if seq.len() > self.pressed_modifiers.len() {
                continue;
            }

            if &self.pressed_modifiers[..seq.len()] == seq {
                return Some(&self.layers[modifier_sequence.target_layer]);
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
            .unwrap_or_else(|| self.get_remapping_current_layer(scan_code));

        self.process_modifiers(scan_code, up);

        if !up {
            self.pressed_keys.insert(scan_code, remap);
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
}
