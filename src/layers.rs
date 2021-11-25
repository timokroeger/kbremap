//! Remapping and layer switching logic.

use std::collections::HashMap;
use std::mem;

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

struct MatchingModifierSequences<'a> {
    base_layer_idx: Option<usize>,
    modifier_sequences: &'a [ModifierSequence],
    pressed_modifers: &'a [u16],
}

impl<'a> Iterator for MatchingModifierSequences<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        for idx in 0..self.pressed_modifers.len() {
            for modifier_seq in self.modifier_sequences {
                let seq = &modifier_seq.sequence;

                debug_assert!(seq.len() > 0);
                if idx + seq.len() > self.pressed_modifers.len() {
                    continue;
                }

                if &self.pressed_modifers[idx..idx + seq.len()] == seq {
                    self.pressed_modifers = &self.pressed_modifers[idx + seq.len()..];
                    return Some(modifier_seq.target_layer);
                }
            }
        }

        return self.base_layer_idx.take();
    }
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

    // Currently active layer.
    active_layer: usize,

    // Previously active layer.
    previous_layer: usize,

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
    for modifier in modifiers {
        if modifier.from != layer_idx {
            continue;
        }

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
    for modifier in modifiers {
        if modifier.from != layer_idx {
            continue;
        }

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
    pub fn new(config: &Config) -> Result<Self> {
        // Virtual keyboard layer activation can be viewed as graph where layers
        // are nodes and modifiers (layer change keys) are egdes.
        let layers = Vec::from_iter(config.layer_names().map(|layer_name| Layer {
            name: String::from(layer_name),
            mappings: config.layer_mappings(layer_name),
        }));

        let mut modifiers = Vec::new();
        for (i, layer_to) in config.layer_names().enumerate() {
            for (j, layer_from) in config.layer_names().enumerate() {
                for (scan_code, target_layer) in config.layer_modifiers(layer_from) {
                    if target_layer != layer_to {
                        continue;
                    }

                    modifiers.push(LayerModifier {
                        scan_code,
                        from: j,
                        to: i,
                    });
                }
            }
        }

        // TODO: Smart way to figure out the base layer.
        let base_layer = layers
            .iter()
            .position(|layer| layer.name == "base")
            .expect("Layer \"base\" not found");

        // Get a set of all modifiers.
        let mut modifiers_scan_codes =
            Vec::from_iter(modifiers.iter().map(|modifier| modifier.scan_code));
        modifiers_scan_codes.dedup();

        let mut this = Self {
            layers,
            modifiers,
            modifiers_scan_codes,
            modifier_sequences: Vec::new(),
            pressed_modifiers: Vec::new(),
            active_layer: base_layer,
            previous_layer: base_layer,
            pressed_keys: HashMap::new(),
        };

        this.build_modifier_sequences(base_layer)?;

        Ok(this)
    }

    /// Build the modifier sequences for layer activation.
    fn build_modifier_sequences(&mut self, base_layer: usize) -> Result<()> {
        // Layer graph validation
        let mut visited = Vec::new();
        let mut finished = Vec::new();
        check_layer_graph(&self.modifiers, base_layer, &mut visited, &mut finished)?;

        self.modifier_sequences.clear();
        self.modifier_sequences.push(ModifierSequence {
            target_layer: base_layer,
            sequence: Vec::new(),
        });
        build_modifier_sequences(&self.modifiers, base_layer, &mut self.modifier_sequences);
        self.modifier_sequences
            .sort_by_key(|modifier_sequence| std::cmp::Reverse(modifier_sequence.sequence.len()));

        Ok(())
    }

    /// Returns iterator over the index of the layers with matching modifier sequences.
    ///
    /// A layer is considered to be active when an chronologically ordered set
    /// of pressed modifer keys matches the layer's activation sequence. This
    /// is true even when modifier keys are removed from the set randomly.
    fn match_modifier_sequences<'a>(
        &'a self,
        pressed_modifers: &'a [u16],
    ) -> MatchingModifierSequences<'a> {
        // Split off the last sequence when matching.
        // It always targets the base layer and has a modifier sequence of length 0.
        let (base_seq, modifier_sequences) = self.modifier_sequences.split_last().unwrap();

        MatchingModifierSequences {
            base_layer_idx: Some(base_seq.target_layer),
            modifier_sequences,
            pressed_modifers,
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

        let mut mod_matcher = self.match_modifier_sequences(&self.pressed_modifiers);
        let active_layer = mod_matcher.next().unwrap();

        // Lock the layer if we find a second sequence for this layer
        // Example: Both shift key pressed to lock the shift layer (caps lock functionality).
        if mod_matcher.any(|layer_idx| layer_idx == active_layer) {
            // Find and reverse the edge between the previous and the new layer
            for modifier in &mut self.modifiers {
                if modifier.from == self.previous_layer && modifier.to == active_layer {
                    mem::swap(&mut modifier.from, &mut modifier.to);
                }
            }

            // Switch the base layer by rebuilding modifier sequences.
            self.build_modifier_sequences(active_layer);
        }

        if active_layer != self.active_layer {
            self.previous_layer = self.active_layer;
            self.active_layer = active_layer;
        }
    }

    /// Returs the remap action associated with the scan code.
    pub fn get_remapping(&mut self, scan_code: u16, up: bool) -> Option<Remap> {
        // Get the active remapping if the key is already pressed so that we can
        // send the correct repeated key press or key up event.
        // If we do not track active key presses the key down and key up events
        // may not be the same if the layer has changed in between.
        let remap = self.pressed_keys.remove(&scan_code).unwrap_or_else(|| {
            self.layers[self.active_layer]
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
}
