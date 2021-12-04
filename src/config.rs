//! Serde based configuration parser.

use std::collections::HashMap;

use serde::Deserialize;

use crate::layout::{KeyAction, Layout, LayoutBuilder};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub caps_lock_layer: Option<String>,

    layers: HashMap<String, Vec<Mapping>>,
}

#[derive(Debug, Deserialize)]
struct Mapping {
    scan_code: u16,
    #[serde(flatten)]
    target: MappingTarget,
}

#[derive(Debug, Deserialize)]
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

    pub fn to_layout(&self) -> Layout {
        let mut layout_builder = LayoutBuilder::new();

        for (layer, mappings) in &self.layers {
            layout_builder.add_or_get_layer(layer);

            for mapping in mappings {
                match &mapping.target {
                    MappingTarget::Characters { characters } if !characters.is_empty() => {
                        for (i, c) in characters.chars().enumerate() {
                            layout_builder.add_key(
                                mapping.scan_code + i as u16,
                                layer,
                                KeyAction::Character(c),
                            );
                        }
                    }
                    MappingTarget::VirtualKeys { virtual_keys } if !virtual_keys.is_empty() => {
                        for (i, vk) in virtual_keys.iter().enumerate() {
                            layout_builder.add_key(
                                mapping.scan_code + i as u16,
                                layer,
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
                            layer,
                            target_layer,
                            *virtual_key,
                        );
                    }
                    MappingTarget::LayerLock {
                        layer_lock: target_layer,
                        virtual_key,
                    } => {
                        layout_builder.add_layer_lock(
                            mapping.scan_code,
                            layer,
                            target_layer,
                            *virtual_key,
                        );
                    }
                    _ => {
                        layout_builder.add_key(mapping.scan_code, layer, KeyAction::Ignore);
                    }
                }
            }
        }
        layout_builder.build()
    }
}
