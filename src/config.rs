use std::collections::HashMap;

use serde::Deserialize;

type LayerMap = HashMap<u16, char>;

pub struct Config {
    base_layer: LayerMap,
    layers: Vec<(Vec<u16>, LayerMap)>,
}

#[derive(Debug, Deserialize)]
struct TopLevelConfig {
    pub layers: HashMap<String, LayerConfig>,
}

#[derive(Debug, Deserialize)]
struct LayerConfig {
    modifiers: Option<Vec<u16>>,
    map: Vec<MapConfig>,
}

#[derive(Debug, Deserialize)]
struct MapConfig {
    scan_code: u16,
    characters: String,
}

impl Config {
    pub fn from_toml(config_str: &str) -> anyhow::Result<Config> {
        let tlc: TopLevelConfig = toml::from_str(&config_str)?;

        let mut config = Config {
            base_layer: HashMap::new(),
            layers: Vec::new(),
        };
        for (_layer_name, layer_config) in &tlc.layers {
            let mut map = HashMap::new();
            for map_config in &layer_config.map {
                for (i, key) in map_config.characters.chars().enumerate() {
                    map.insert(map_config.scan_code + i as u16, key);
                }
            }

            if let Some(mods) = &layer_config.modifiers {
                config.layers.push((mods.clone(), map));
            } else {
                anyhow::ensure!(
                    config.base_layer.is_empty(),
                    "Missing `modifiers` field. There can only be one base layer without modifiers",
                );
                config.base_layer.extend(map);
            }
        }
        Ok(config)
    }

    pub fn is_layer_modifier(&self, scan_code: u16) -> bool {
        self.layer_map(scan_code).is_some()
    }

    pub fn base_layer_map(&self) -> &LayerMap {
        &self.base_layer
    }

    pub fn layer_map(&self, modifier_scan_code: u16) -> Option<&LayerMap> {
        self.layers
            .iter()
            .find(|(modifiers, _)| modifiers.contains(&modifier_scan_code))
            .map(|(_, map)| map)
    }
}
