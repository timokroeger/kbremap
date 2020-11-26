use std::collections::HashMap;

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;

use crate::KeyAction;
use crate::{keyboard_hook::Remap, LayerMap};

#[derive(Debug, Deserialize)]
pub struct Config {
    layers: HashMap<String, Vec<MappingConfig>>,

    #[serde(skip)]
    result: HashMap<String, LayerMap>,
}

#[derive(Debug, Deserialize)]
struct MappingConfig {
    scan_code: u16,
    layer: Option<String>,
    characters: Option<String>,
}

impl Config {
    pub fn from_toml(config_str: &str) -> Result<HashMap<String, LayerMap>> {
        let config: Config = toml::from_str(&config_str)?;

        let result = parse_layer(&config, "base", HashMap::new())?;
        for (layer_name, _) in &config.layers {
            if !result.contains_key(layer_name) {
                println!("Warning: Ignoring unreferenced layer `{}`", layer_name);
            }
        }

        Ok(result)
    }
}

fn parse_layer(
    config: &Config,
    layer_name: &str,
    mut result: HashMap<String, LayerMap>,
) -> Result<HashMap<String, LayerMap>> {
    // Check if layer was processed already.
    if result.contains_key(layer_name) {
        return Ok(result);
    }

    let layer_config = config
        .layers
        .get(layer_name)
        .context("Invalid layer reference")?;

    let mut map = HashMap::new();
    for mapping in layer_config {
        match (&mapping.layer, &mapping.characters) {
            (Some(target_layer), None) => {
                ensure!(
                    config.layers.contains_key(target_layer),
                    "Invalid layer reference `{}`",
                    target_layer
                );
                map.insert(
                    mapping.scan_code,
                    KeyAction::Layer(Remap::Ignore, target_layer.clone()),
                );

                result = parse_layer(config, target_layer, result)?;

                // Also insert a layer action in the target layer to get back.
                result.get_mut(target_layer).unwrap().insert(
                    mapping.scan_code,
                    KeyAction::Layer(Remap::Ignore, layer_name.to_string()),
                );
            }
            (None, Some(characters)) => {
                for (i, c) in characters.chars().enumerate() {
                    map.insert(
                        mapping.scan_code + i as u16,
                        KeyAction::Remap(Remap::Character(c)),
                    );
                }
            }
            _ => bail!("Invalid config"), // TODO: Improve error handling
        }
    }
    result.insert(layer_name.to_string(), map);
    Ok(result)
}
