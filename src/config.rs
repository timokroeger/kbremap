//! Serde based configuration parser.

use std::collections::HashMap;

use serde::Deserialize;

use crate::keyboard_hook::Remap;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub disable_caps_lock: bool,

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
}

impl Config {
    pub fn from_toml(config_str: &str) -> Result<Config, toml::de::Error> {
        let config = toml::from_str(config_str)?;
        Ok(config)
    }

    pub fn layer_names(&self) -> impl Iterator<Item = &str> {
        self.layers.keys().map(String::as_str)
    }

    pub fn layer_mappings(&self, layer_name: &str) -> HashMap<u16, Remap> {
        let mut mappings = HashMap::new();
        for mapping in &self.layers[layer_name] {
            let mut insert_mapping = |scan_code, remap| {
                if let Some(prev_remap) = mappings.insert(scan_code, remap) {
                    println!(
                        "Warning: `{:?}` overwritten by `{:?}` for scan_code={:#06X}",
                        prev_remap, remap, mapping.scan_code
                    );
                }
            };

            match &mapping.target {
                MappingTarget::Characters { characters } if !characters.is_empty() => {
                    for (i, c) in characters.chars().enumerate() {
                        insert_mapping(mapping.scan_code + i as u16, Remap::Character(c));
                    }
                }
                MappingTarget::VirtualKeys { virtual_keys } if !virtual_keys.is_empty() => {
                    for (i, vk) in virtual_keys.iter().enumerate() {
                        insert_mapping(mapping.scan_code + i as u16, Remap::VirtualKey(*vk));
                    }
                }
                MappingTarget::Layer {
                    virtual_key: Some(vk),
                    ..
                } => {
                    insert_mapping(mapping.scan_code, Remap::VirtualKey(*vk));
                }
                _ => {
                    insert_mapping(mapping.scan_code, Remap::Ignore);
                }
            }
        }
        mappings
    }

    pub fn layer_modifiers(&self, layer_name: &str) -> impl Iterator<Item = (u16, &str)> {
        self.layers[layer_name].iter().filter_map(|mapping| {
            if let MappingTarget::Layer { layer, .. } = &mapping.target {
                Some((mapping.scan_code, layer.as_str()))
            } else {
                None
            }
        })
    }
}
