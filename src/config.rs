//! Serde based configuration parser.

use std::collections::HashMap;

use anyhow::{Result, bail, ensure};
use serde::Deserialize;

use crate::layout::{KeyAction, Layout};

#[derive(Debug, Deserialize)]
struct ReadableConfig {
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

#[derive(Debug)]
pub struct Config {
    pub caps_lock_layer: Option<String>,
    pub layout: Layout,
}

pub fn parse_config_toml(config: &str) -> Result<Config> {
    let mut config: ReadableConfig = toml::from_str(config)?;

    if let Some(caps_lock_layer) = &config.caps_lock_layer {
        ensure!(
            config.layers.contains_key(caps_lock_layer),
            "caps lock layer not found"
        );
    }

    let mut layout = Layout::new();
    let mut name_to_idx = HashMap::new();
    let mut mappings = Vec::new();

    let mut add_layer = |name: String, mapping| {
        let layer_idx = layout.add_layer(name.clone());
        name_to_idx.insert(name, layer_idx);
        mappings.push((layer_idx, mapping));
    };

    // Base layer must be added first.
    let Some(mapping) = config.layers.remove(&config.base_layer) else {
        bail!("base layer not found");
    };
    add_layer(config.base_layer, mapping);

    // First pass: add layers and track their indices.
    for (name, mapping) in config.layers {
        add_layer(name, mapping)
    }

    // Second pass: add mappings.
    for (layer_idx, mappings) in mappings {
        for mapping in mappings {
            match &mapping.target {
                MappingTarget::Characters { characters } if !characters.is_empty() => {
                    for (i, c) in characters.chars().enumerate() {
                        layout.add_key(
                            mapping.scan_code + i as u16,
                            layer_idx,
                            KeyAction::Character(c),
                        );
                    }
                }
                MappingTarget::VirtualKeys { virtual_keys } if !virtual_keys.is_empty() => {
                    for (i, vk) in virtual_keys.iter().enumerate() {
                        layout.add_key(
                            mapping.scan_code + i as u16,
                            layer_idx,
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
                            layer_idx,
                            name_to_idx[target_layer],
                        );
                    }

                    if let Some(lock_layer) = lock_layer {
                        layout.add_layer_lock(
                            mapping.scan_code,
                            layer_idx,
                            name_to_idx[lock_layer],
                        );
                    }

                    layout.add_key(
                        mapping.scan_code,
                        layer_idx,
                        virtual_key.map_or(KeyAction::Ignore, KeyAction::VirtualKey),
                    );
                }
                _ => {
                    layout.add_key(mapping.scan_code, layer_idx, KeyAction::Ignore);
                }
            }
        }
    }

    Ok(Config {
        caps_lock_layer: config.caps_lock_layer,
        layout,
    })
}
