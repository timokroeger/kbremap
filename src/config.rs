//! Serde based configuration parser.

use std::collections::HashMap;

use serde::Deserialize;
use thiserror::Error;

use crate::layout::{self, KeyAction, Layout};

#[derive(Debug, Deserialize)]
pub struct ReadableConfig {
    caps_lock_layer: Option<String>,
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

#[derive(Debug)]
pub struct Config {
    pub caps_lock_layer: Option<String>,
    pub layout: Layout,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("caps lock layer not found")]
    InvalidCapsLockLayer,
    #[error("layout")]
    Layout(#[from] layout::Error),
}

impl TryFrom<ReadableConfig> for Config {
    type Error = ConfigError;

    fn try_from(config: ReadableConfig) -> Result<Self, Self::Error> {
        let mut layout = Layout::new();

        if let Some(caps_lock_layer) = &config.caps_lock_layer {
            if !config.layers.contains_key(caps_lock_layer) {
                return Err(ConfigError::InvalidCapsLockLayer);
            }
        }

        let mut layers = HashMap::with_capacity(config.layers.len());
        for (name, mapping) in config.layers.into_iter() {
            layers.insert(name.clone(), (layout.add_layer(name), mapping));
        }

        for (layer_idx, mappings) in layers.values() {
            for mapping in mappings {
                match &mapping.target {
                    MappingTarget::Characters { characters } if !characters.is_empty() => {
                        for (i, c) in characters.chars().enumerate() {
                            layout.add_key(
                                mapping.scan_code + i as u16,
                                *layer_idx,
                                KeyAction::Character(c),
                            );
                        }
                    }
                    MappingTarget::VirtualKeys { virtual_keys } if !virtual_keys.is_empty() => {
                        for (i, vk) in virtual_keys.iter().enumerate() {
                            layout.add_key(
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
                        layout.add_modifier(
                            mapping.scan_code,
                            *layer_idx,
                            layers[target_layer].0,
                            virtual_key.clone(),
                        );
                    }
                    MappingTarget::LayerLock {
                        layer_lock: target_layer,
                        virtual_key,
                    } => {
                        layout.add_layer_lock(
                            mapping.scan_code,
                            *layer_idx,
                            layers[target_layer].0,
                            virtual_key.clone(),
                        );
                    }
                    _ => {
                        layout.add_key(mapping.scan_code, *layer_idx, KeyAction::Ignore);
                    }
                }
            }
        }
        layout.finalize()?;

        Ok(Self {
            caps_lock_layer: config.caps_lock_layer,
            layout,
        })
    }
}
