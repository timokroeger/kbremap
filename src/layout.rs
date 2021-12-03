/// Byte 0 of [`Key::action`] contains the virtual key.
const TAG_VIRTUAL_KEY: u8 = 0;

/// [`Key::action`] are the bytes of a unicode code point.
const TAG_CHARACTER: u8 = 1;

/// Changes the active layer. Byte 3 of [`Key::action`] contains the target layer.
const TAG_MODIFIER: u8 = 2;

/// Locks the current layer. Byte 3 of [`Key::action`] contains the target layer.
const TAG_LAYER_LOCK: u8 = 3;

/// Action associated with the key. Returned by the user provided hook callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Do not forward or send a key action.
    Ignore,

    /// Sends a (Unicode) character, if possible as virtual key press.
    Character(char),

    /// Sends a virtual key press.
    /// Reference: <https://docs.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
    VirtualKey(u8),
}

/// Compact representation of a key action.
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Default)]
pub struct LayoutBuilder {
    keys: Vec<Key>,
    layer_names: Vec<String>,
}

impl LayoutBuilder {
    pub fn new() -> Self {
        Self::default()
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

    pub fn add_layer_lock(
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
            tag: TAG_LAYER_LOCK,
            action: [vk.unwrap_or(0), 0, 0, target_layer],
        };
        self.keys.push(key);
        self
    }

    pub fn build(mut self) -> Layout {
        self.keys.sort_by_key(|k| k.scan_code);
        Layout { keys: self.keys }
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
}

impl Layout {
    pub fn get_key(&self, scan_code: u16) -> KeyResults<'_> {
        KeyResults {
            keys: &self.keys,
            idx: self
                .keys
                .binary_search_by_key(&scan_code, |k| k.scan_code)
                .unwrap_or_else(|idx| idx),
            scan_code,
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

    pub fn layer_locks(&self) -> impl Iterator<Item = Modifier> + '_ {
        self.keys
            .iter()
            .filter(|k| k.tag == TAG_LAYER_LOCK)
            .map(|k| Modifier {
                scan_code: k.scan_code,
                layer_from: k.layer,
                layer_to: k.action[3],
            })
    }
}

pub struct KeyResults<'a> {
    keys: &'a [Key],
    idx: usize,
    scan_code: u16,
}

impl<'a> KeyResults<'a> {
    fn key_on_layer(&self, layer: u8) -> Option<Key> {
        let iter_back = self.keys[..self.idx]
            .iter()
            .rev()
            .take_while(|k| k.scan_code == self.scan_code);
        let iter_forward = self.keys[self.idx..]
            .iter()
            .take_while(|k| k.scan_code == self.scan_code);
        iter_back
            .chain(iter_forward)
            .find(|k| k.layer == layer)
            .copied()
    }

    pub fn action(&self, layer: u8) -> Option<KeyAction> {
        let key = self.key_on_layer(layer)?;
        let action = match key.tag {
            TAG_VIRTUAL_KEY | TAG_MODIFIER | TAG_LAYER_LOCK => match key.action[0] {
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

    pub fn layer_lock(&self, layer: u8) -> Option<u8> {
        let key = self.key_on_layer(layer)?;
        match key.tag {
            TAG_LAYER_LOCK => Some(key.action[3]),
            _ => None,
        }
    }
}
