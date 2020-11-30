use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};

use crate::keyboard_hook::Remap;

#[derive(Debug)]
pub struct LayerMap {
    keys: HashMap<u16, Remap>,
    layer_modifiers: HashMap<u16, String>, // TODO: Remove
}

impl LayerMap {
    pub fn new() -> LayerMap {
        LayerMap {
            keys: HashMap::new(),
            layer_modifiers: HashMap::new(),
        }
    }

    pub fn add_key(&mut self, scan_code: u16, remap: Remap) -> Result<()> {
        if let Some(remap) = self.keys.get(&scan_code) {
            bail!("Scan code {:#06X} already mapped to {:?}", scan_code, remap);
        }
        self.keys.insert(scan_code, remap);
        Ok(())
    }

    pub fn add_layer_modifier(
        &mut self,
        scan_code: u16,
        remap: Remap,
        target_layer_name: &str,
    ) -> Result<()> {
        self.add_key(scan_code, remap)?;
        self.layer_modifiers
            .insert(scan_code, target_layer_name.to_string());
        Ok(())
    }
}

/// Mapping table for a virtual keyboard layer.
#[derive(Debug)]
struct Layer {
    name: String,

    map: LayerMap,

    /// Sequences of modifier keys that activate this layer.
    activation_sequences: Vec<Vec<u16>>,
}

/// Collection of virtual keyboard layer and logic to switch between them
/// depending on which modifier keys are pressed.
#[derive(Debug)]
pub struct Layers {
    layers: Vec<Layer>,

    /// Index to the base layer (layer without modifiers)
    base_layer_idx: usize,

    /// Keys used for layer switching.
    modifiers: HashSet<u16>,

    /// Currently pressed layer modifiers keys.
    pressed_modifiers: Vec<u16>,

    /// Currently pressed keys.
    pressed_keys: HashMap<u16, Remap>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            layers: Vec::new(),
            base_layer_idx: 0,
            modifiers: HashSet::new(),
            pressed_modifiers: Vec::new(),
            pressed_keys: HashMap::new(),
        }
    }

    pub fn add_layer(&mut self, name: &str, map: LayerMap) {
        self.layers.push(Layer {
            name: name.to_string(),
            activation_sequences: Vec::new(),
            map,
        })
    }

    fn get_layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    fn get_layer_mut(&mut self, name: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.name == name)
    }

    pub fn has_layer(&self, name: &str) -> bool {
        self.get_layer(name).is_some()
    }

    // TODO: Can this be improved especially in regard to cloning dropping?
    /// Virtual keyboard layer activation can be viewed as graph where layers
    /// are nodes and layer action keys are egdes. Traverses the graph starting
    /// at the base layer and stores the path (set of edges) to each layer as
    /// activation sequence.
    pub fn build_activation_sequences(&mut self, layer_name: &str) {
        let layer = self.get_layer(layer_name).unwrap();

        let activation_sequences = layer.activation_sequences.clone();

        for (&scan_code, target_layer_name) in &layer.map.layer_modifiers.clone() {
            self.modifiers.insert(scan_code);

            let target_layer = self.get_layer_mut(&target_layer_name).unwrap();

            if activation_sequences.is_empty() {
                target_layer.activation_sequences.push(vec![scan_code]);
            } else {
                for mut seq in activation_sequences.clone() {
                    seq.push(scan_code);
                    target_layer.activation_sequences.push(seq);
                }
            }

            let target_layer_name = target_layer.name.clone();
            self.build_activation_sequences(&target_layer_name);
        }

        if activation_sequences.is_empty() {
            self.base_layer_idx = self
                .layers
                .iter()
                .position(|l| l.name == layer_name)
                .unwrap();
        }
    }

    /// Returns the currently active layer or `None` when no layer is active.
    ///
    /// A layer is considered to be active when an chronologically ordered set
    /// of pressed modifer keys matches the layer's activation sequence. This
    /// is true even when modifier keys are removed from the set randomly.
    fn active_layer(&self) -> Option<&Layer> {
        if self.pressed_modifiers.is_empty() {
            return self.layers.get(self.base_layer_idx);
        }

        for layer in &self.layers {
            if layer.activation_sequences.contains(&self.pressed_modifiers) {
                return Some(layer);
            }
        }

        None
    }

    /// Processes modifers to update select the correct layer.
    fn process_modifiers(&mut self, scan_code: u16, up: bool) {
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

    pub fn get_remapping(&mut self, scan_code: u16, up: bool) -> Remap {
        let remap = match self.active_layer() {
            Some(layer) => match layer.map.keys.get(&scan_code) {
                Some(r) => *r,
                None => Remap::Transparent,
            },
            None => Remap::Ignore,
        };

        if self.modifiers.contains(&scan_code) {
            self.process_modifiers(scan_code, up);
        }

        if let Some(&remap) = self.pressed_keys.get(&scan_code) {
            if up {
                self.pressed_keys.remove(&scan_code);
            }
            return remap;
        }

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
        let mut layers = Layers::new();

        let mut l0 = LayerMap::new();
        l0.add_layer_modifier(0x11, Remap::Ignore, "l1")?;
        l0.add_layer_modifier(0x12, Remap::Ignore, "l2")?;
        l0.add_key(0x20, Remap::Character('0'))?;
        layers.add_layer("l0", l0);

        let mut l1 = LayerMap::new();
        l1.add_layer_modifier(0x12, Remap::Ignore, "l3")?;
        l1.add_key(0x20, Remap::Character('1'))?;
        layers.add_layer("l1", l1);

        let mut l2 = LayerMap::new();
        l2.add_key(0x20, Remap::Character('2'))?;
        layers.add_layer("l2", l2);

        let mut l3 = LayerMap::new();
        l3.add_key(0x20, Remap::Character('3'))?;

        // Adding an existing key should fail.
        l3.add_key(0x20, Remap::Character('X')).unwrap_err();

        layers.add_layer("l3", l3);

        layers.build_activation_sequences("l0");

        // L0
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        // L1
        assert_eq!(layers.get_remapping(0x11, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x11, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        // L2
        assert_eq!(layers.get_remapping(0x12, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x12, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        // L1 -> L3 -> L2
        assert_eq!(layers.get_remapping(0x11, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x12, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('3'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('3'));
        assert_eq!(layers.get_remapping(0x11, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x12, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        // L2 -> XX -> L1
        assert_eq!(layers.get_remapping(0x12, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('2'));
        assert_eq!(layers.get_remapping(0x11, false), Remap::Transparent);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x12, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x11, true), Remap::Transparent);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        // Change layer during key press
        assert_eq!(layers.get_remapping(0x11, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x11, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

        Ok(())
    }
}
