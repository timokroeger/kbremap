//! Serde based configuration parser.

use std::collections::HashMap;

use serde::Deserialize;
use thiserror::Error;

use crate::layout::{KeyAction, LayerIdx, LayoutBuilder, LayoutStorage};

pub const INVALID_LAYER_IDX: LayerIdx = LayerIdx::MAX;

#[derive(Debug, Deserialize)]
pub struct ReadableConfig {
    base_layer: String,
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

pub struct Config {
    pub caps_lock_layer_idx: LayerIdx,
    pub layout: LayoutStorage,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("base layer not found")]
    InvalidBaseLayer,
    #[error("caps lock layer not found")]
    InvalidCapsLockLayer,
}

impl TryFrom<ReadableConfig> for Config {
    type Error = ConfigError;

    fn try_from(config: ReadableConfig) -> Result<Self, Self::Error> {
        let mut layout = LayoutBuilder::new();

        if !config.layers.contains_key(&config.base_layer) {
            return Err(ConfigError::InvalidBaseLayer);
        }

        if let Some(caps_lock_layer) = &config.caps_lock_layer
            && !config.layers.contains_key(caps_lock_layer)
        {
            return Err(ConfigError::InvalidCapsLockLayer);
        }

        // Base layer must be added first.
        let base_layer_idx = layout.add_layer();
        let mut caps_lock_layer_idx = INVALID_LAYER_IDX;
        let mut layers = HashMap::with_capacity(config.layers.len());

        for (name, mapping) in config.layers {
            let layer_idx = if name == config.base_layer {
                base_layer_idx
            } else {
                layout.add_layer()
            };
            if let Some(caps_lock_layer) = &config.caps_lock_layer
                && name == *caps_lock_layer
            {
                caps_lock_layer_idx = layer_idx;
            }
            layers.insert(name, (layer_idx, mapping));
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
                            virtual_key.map_or(KeyAction::Ignore, KeyAction::VirtualKey),
                        );
                    }
                    _ => {
                        layout.add_key(mapping.scan_code, *layer_idx, KeyAction::Ignore);
                    }
                }
            }
        }

        Ok(Self {
            caps_lock_layer_idx,
            layout: layout.build(),
        })
    }
}
