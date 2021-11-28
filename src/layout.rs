use crate::keyboard_hook::KeyAction;

const TAG_VIRTUAL_KEY: u8 = 0;
const TAG_CHARACTER: u8 = 1;
const TAG_MODIFIER: u8 = 2; // `virtual_keys[3]` contains the target layer

#[derive(Debug)]
struct Key {
    scan_code: u16,
    layer: u8,
    tag: u8,
    action: [u8; 4],
}

impl Key {
    fn from_action(scan_code: u16, layer: u8, ka: KeyAction) -> Self {
        let (tag, action) = match ka {
            KeyAction::Ignore => (TAG_VIRTUAL_KEY, [0; 4]),
            KeyAction::VirtualKey(vk) => (TAG_VIRTUAL_KEY, [vk, 0, 0, 0]),
            KeyAction::Character(c) => (TAG_CHARACTER, u32::from(c).to_ne_bytes()),
        };

        Self {
            scan_code,
            layer,
            tag,
            action,
        }
    }
}

#[derive(Debug)]
pub struct LayouBuilder {
    keys: Vec<Key>,
    layer_names: Vec<String>,
}

impl LayouBuilder {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            layer_names: Vec::new(),
        }
    }

    fn add_or_get_layer(&mut self, layer: &str) -> u8 {
        let layer_idx = self
            .layer_names
            .iter()
            .position(|l| l.as_str() == layer)
            .unwrap_or_else(|| {
                self.layer_names.push(layer.to_string());
                self.layer_names.len() - 1
            });
        layer_idx.try_into().expect("max 256 layers supported")
    }

    pub fn add_key(&mut self, scan_code: u16, layer: &str, action: KeyAction) -> &mut Self {
        let layer = self.add_or_get_layer(layer);
        self.keys.push(Key::from_action(scan_code, layer, action));
        self
    }

    pub fn add_modifier(
        &mut self,
        scan_code: u16,
        layer: &str,
        target_layer: &str,
        vk: Option<u8>,
    ) -> &mut Self {
        let layer = self.add_or_get_layer(layer);
        let target_layer = self.add_or_get_layer(target_layer);
        let key = Key {
            scan_code,
            layer,
            tag: TAG_MODIFIER,
            action: [vk.unwrap_or(0), 0, 0, target_layer],
        };
        self.keys.push(key);
        self
    }

    pub fn build(mut self) -> Layout {
        self.keys.sort_by_key(|k| k.scan_code);
        Layout {
            keys: self.keys,
            layer_names: self.layer_names,
        }
    }
}

#[derive(Debug)]
pub struct Modifier {
    pub scan_code: u16,
    pub layer_from: u8,
    pub layer_to: u8,
}

#[derive(Debug)]
pub struct Layout {
    keys: Vec<Key>,
    layer_names: Vec<String>,
}

impl Layout {
    pub fn get_key(&self, scan_code: u16) -> KeyResults<'_> {
        let idx = self
            .keys
            .binary_search_by_key(&scan_code, |k| k.scan_code)
            .unwrap_or_else(|idx| idx);
        let len = self.keys[idx..]
            .iter()
            .take_while(|k| k.scan_code == scan_code)
            .count();
        KeyResults {
            keys: &self.keys[idx..idx + len],
        }
    }

    pub fn modifiers(&self) -> impl Iterator<Item = Modifier> + '_ {
        self.keys
            .iter()
            .filter(|k| k.tag == TAG_MODIFIER)
            .map(|k| Modifier {
                scan_code: k.scan_code,
                layer_from: k.layer,
                layer_to: k.action[3],
            })
    }
}

pub struct KeyResults<'a> {
    keys: &'a [Key],
}

impl<'a> KeyResults<'a> {
    pub fn action_on_layer(&self, layer: u8) -> Option<KeyAction> {
        let key = self.keys.iter().find(|k| k.layer == layer)?;
        let action = match key.tag {
            TAG_VIRTUAL_KEY | TAG_MODIFIER => match key.action[0] {
                0 => KeyAction::Ignore,
                vk => KeyAction::VirtualKey(vk),
            },
            TAG_CHARACTER => {
                KeyAction::Character(char::from_u32(u32::from_ne_bytes(key.action)).unwrap())
            }
            _ => unreachable!(),
        };
        Some(action)
    }
}
