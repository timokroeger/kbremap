use std::collections::HashMap;

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;

use crate::{
    keyboard_hook::Remap,
    layers::{LayerMap, Layers},
};

#[derive(Debug, Deserialize)]
pub struct Config {
    layers: HashMap<String, Vec<MappingConfig>>,
}

#[derive(Debug, Deserialize)]
struct MappingConfig {
    scan_code: u16,
    layer: Option<String>,
    characters: Option<String>,
    forward_scan_code: Option<bool>,
}

impl Config {
    pub fn from_toml(config_str: &str) -> Result<Layers> {
        let config: Config = toml::from_str(&config_str)?;

        let mut layers = Layers::new();

        let base_layer_name = String::from("base");
        let base_layer_map = parse_layer_map(&config, &base_layer_name, &mut layers)?;
        layers.add_layer(base_layer_name, base_layer_map);

        for (layer_name, _) in &config.layers {
            if !layers.has_layer(layer_name) {
                println!("Warning: Ignoring unreferenced layer `{}`", layer_name);
            }
        }

        Ok(layers)
    }
}

// Recursive function to parse all layers that can be reached by layer activation keys.
fn parse_layer_map(config: &Config, layer_name: &str, layers: &mut Layers) -> Result<LayerMap> {
    let layer_config = config
        .layers
        .get(layer_name)
        .context("Invalid layer reference")?;

    let mut map = LayerMap::new();
    for mapping in layer_config {
        match (&mapping.layer, &mapping.characters) {
            (Some(target_layer_name), None) => {
                ensure!(
                    config.layers.contains_key(target_layer_name),
                    "Invalid layer reference `{}`",
                    target_layer_name
                );

                let remap = if mapping.forward_scan_code == Some(true) {
                    Remap::Transparent
                } else {
                    Remap::Ignore
                };

                map.add_layer_modifier(mapping.scan_code, remap, target_layer_name);

                // Build the target layer map if not available already.
                if !layers.has_layer(target_layer_name) {
                    let target_layer_map = parse_layer_map(config, target_layer_name, layers)?;
                    layers.add_layer(target_layer_name.clone(), target_layer_map);
                }
            }
            (None, Some(characters)) => {
                for (i, c) in characters.chars().enumerate() {
                    let remap = if c == '\0' {
                        Remap::Ignore
                    } else {
                        Remap::Character(c)
                    };
                    map.add_key(mapping.scan_code + i as u16, remap);
                }
            }
            _ => bail!("Invalid config"), // TODO: Improve error handling
        }
    }

    Ok(map)
}
