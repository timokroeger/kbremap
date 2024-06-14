//! Serde based configuration parser.

use std::collections::HashMap;

use serde::Deserialize;

use crate::layout::{KeyAction, Layout};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    caps_lock_layer: Option<String>,
    layers: HashMap<String, Vec<Mapping>>,
}

#[derive(Debug, Deserialize, Clone)]
struct Mapping {
    scan_code: u16,
    #[serde(flatten)]
    target: MappingTarget,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum MappingTarget {
    Characters {
        characters: String,
    },
    VirtualKeys {
        virtual_keys: Vec<u8>,
    },
    Layer {
        layer: String,
        virtual_key: Option<u8>,
    },
    LayerLock {
        layer_lock: String,
        virtual_key: Option<u8>,
    },
}

impl Config {
    pub fn from_toml(config_str: &str) -> Result<Config, toml::de::Error> {
        let config = toml::from_str(config_str)?;
        Ok(config)
    }

    pub fn into_layout(self) -> Layout {
        let mut layout_builder = Layout::new();

        let mut layers = HashMap::with_capacity(self.layers.len());
        let mut layer_mappings = Vec::with_capacity(self.layers.len());
        for (name, mapping) in self.layers.into_iter() {
            layers.insert(name.clone(), layout_builder.add_layer(name));
            layer_mappings.push(mapping);
        }

        for (layer_idx, mappings) in layers.values().zip(layer_mappings.into_iter()) {
            for mapping in mappings {
                match mapping.target {
                    MappingTarget::Characters { characters } if !characters.is_empty() => {
                        for (i, c) in characters.chars().enumerate() {
                            layout_builder.add_key(
                                mapping.scan_code + i as u16,
                                *layer_idx,
                                KeyAction::Character(c),
                            );
                        }
                    }
                    MappingTarget::VirtualKeys { virtual_keys } if !virtual_keys.is_empty() => {
                        for (i, vk) in virtual_keys.iter().enumerate() {
                            layout_builder.add_key(
                                mapping.scan_code + i as u16,
                                *layer_idx,
                                KeyAction::VirtualKey(*vk),
                            );
                        }
                    }
                    MappingTarget::Layer {
                        layer: target_layer,
                        virtual_key,
                    } => {
                        layout_builder.add_modifier(
                            mapping.scan_code,
                            *layer_idx,
                            layers[&target_layer],
                            virtual_key,
                        );
                    }
                    MappingTarget::LayerLock {
                        layer_lock: target_layer,
                        virtual_key,
                    } => {
                        layout_builder.add_layer_lock(
                            mapping.scan_code,
                            *layer_idx,
                            layers[&target_layer],
                            virtual_key,
                        );
                    }
                    _ => {
                        layout_builder.add_key(mapping.scan_code, *layer_idx, KeyAction::Ignore);
                    }
                }
            }
        }
        layout_builder
    }

    pub fn caps_lock_layer(&self) -> Option<&str> {
        self.caps_lock_layer.as_deref()
    }
}
