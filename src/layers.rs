use std::collections::HashMap;

use crate::keyboard_hook::{KeyboardEvent, Remap};

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

    /// Keys used for layer switching.
    modifiers: HashMap<u16, Remap>,

    /// Currently active modifiers.
    active_modifiers: Vec<u16>,
}

impl Layers {
    pub fn new() -> Layers {
        Layers {
            layers: Vec::new(),
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
    }

    /// Returns the currently active layer or `None` when no layer is active.
    ///
    /// A layer is considered to be active when an chronologically ordered set
    /// of pressed modifer keys matches the layer's activation sequence. This
    /// is true even when modifier keys are removed from the set randomly.
    fn active_layer(&self) -> Option<&Layer> {
        if self.active_modifiers.is_empty() {
            return Some(self.get_layer("base").unwrap());
        }
        for layer in &self.layers {
            if layer.activation_sequences.contains(&self.active_modifiers) {
                return Some(layer);
            }
        }
        None
    }

    /// Processes modifers to update select the correct layer.
    pub fn process_modifiers(&mut self, key: &KeyboardEvent) {
        if self.modifiers.get(&key.scan_code()).is_none() {
            return;
        }

        let active_idx = self
            .active_modifiers
            .iter()
            .rposition(|&scan_code| key.scan_code() == scan_code);
        match active_idx {
            None if key.down() => {
                self.active_modifiers.push(key.scan_code());
            }
            Some(idx) if key.up() => {
                self.active_modifiers.remove(idx);
            }
            _ => {} // Ignore repeated key presses
        }
    }

    pub fn get_remapping(&self, scan_code: u16) -> Remap {
        if let Some(remap) = self.modifiers.get(&scan_code) {
            return *remap;
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
