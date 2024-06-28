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
        layer: Option<String>,
        lock: Option<String>,
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
                        lock: lock_layer,
                        virtual_key,
                    } => {
                        if let Some(target_layer) = target_layer {
                            layout.add_modifier(
                                mapping.scan_code,
                                *layer_idx,
                                layers[target_layer].0,
                            );
                        }

                        if let Some(lock_layer) = lock_layer {
                            layout.add_layer_lock(
                                mapping.scan_code,
                                *layer_idx,
                                layers[lock_layer].0,
                            );
                        }

                        layout.add_key(
                            mapping.scan_code,
                            *layer_idx,
                            virtual_key.map_or_else(|| KeyAction::Ignore, KeyAction::VirtualKey),
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
