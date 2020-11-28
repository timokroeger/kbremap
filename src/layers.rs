use std::collections::HashMap;

use crate::keyboard_hook::Remap;

#[derive(Debug)]
pub enum KeyAction {
    Remap(Remap),
    Layer(Remap, String),
}

pub type LayerMap = HashMap<u16, KeyAction>;

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
    modifiers: HashMap<u16, Remap>,

    /// Currently active modifiers.
    active_modifiers: Vec<u16>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            layers: Vec::new(),
            base_layer_idx: 0,
            modifiers: HashMap::new(),
            active_modifiers: Vec::new(),
        }
    }

    pub fn add_layer(&mut self, name: String, map: LayerMap) {
        self.layers.push(Layer {
            name,
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

        let mut target_layers = Vec::new();
        for (&scan_code, action) in &layer.map {
            if let KeyAction::Layer(remap, target_layer_name) = action {
                target_layers.push((target_layer_name.clone(), scan_code, *remap));
            }
        }

        let activation_sequences = layer.activation_sequences.clone();

        for (target_layer_name, scan_code, remap) in target_layers {
            self.modifiers.insert(scan_code, remap);

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
        if self.active_modifiers.is_empty() {
            return self.layers.get(self.base_layer_idx);
        }

        for layer in &self.layers {
            if layer.activation_sequences.contains(&self.active_modifiers) {
                return Some(layer);
            }
        }

        None
    }

    /// Processes modifers to update select the correct layer.
    fn process_modifiers(&mut self, scan_code: u16, up: bool) {
        let active_idx = self
            .active_modifiers
            .iter()
            .rposition(|&active_sc| active_sc == scan_code);
        match (active_idx, up) {
            (None, false) => {
                self.active_modifiers.push(scan_code);
            }
            (Some(idx), true) => {
                self.active_modifiers.remove(idx);
            }
            _ => {} // Ignore repeated key presses
        }
    }

    pub fn get_remapping(&mut self, scan_code: u16, up: bool) -> Remap {
        if let Some(&remap) = self.modifiers.get(&scan_code) {
            self.process_modifiers(scan_code, up);
            return remap;
        }

        match self.active_layer() {
            Some(layer) => match layer.map.get(&scan_code) {
                Some(KeyAction::Remap(r)) => *r,
                Some(KeyAction::Layer(_, _)) => unreachable!(), // Handled above
                None => Remap::Transparent,
            },
            None => Remap::Ignore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_activation() {
        unsafe {
            use winapi::um::wincon::*;
            AttachConsole(ATTACH_PARENT_PROCESS);
        };

        let mut layers = Layers::new();

        let mut l0 = HashMap::new();
        l0.insert(0x11, KeyAction::Layer(Remap::Ignore, String::from("l1")));
        l0.insert(0x12, KeyAction::Layer(Remap::Ignore, String::from("l2")));
        l0.insert(0x20, KeyAction::Remap(Remap::Character('0')));
        layers.add_layer(String::from("l0"), l0);

        let mut l1 = HashMap::new();
        l1.insert(0x12, KeyAction::Layer(Remap::Ignore, String::from("l3")));
        l1.insert(0x20, KeyAction::Remap(Remap::Character('1')));
        layers.add_layer(String::from("l1"), l1);

        let mut l2 = HashMap::new();
        l2.insert(0x20, KeyAction::Remap(Remap::Character('2')));
        layers.add_layer(String::from("l2"), l2);

        let mut l3 = HashMap::new();
        l3.insert(0x20, KeyAction::Remap(Remap::Character('3')));
        layers.add_layer(String::from("l3"), l3);

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

        // TODO: Fix
        // assert_eq!(layers.get_remapping(0x11, false), Remap::Ignore);
        // assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        // assert_eq!(layers.get_remapping(0x11, true), Remap::Ignore);
        // assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        // assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        // assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));

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
        assert_eq!(layers.get_remapping(0x11, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x12, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('1'));
        assert_eq!(layers.get_remapping(0x11, true), Remap::Ignore);
        assert_eq!(layers.get_remapping(0x20, false), Remap::Character('0'));
        assert_eq!(layers.get_remapping(0x20, true), Remap::Character('0'));
    }
}
