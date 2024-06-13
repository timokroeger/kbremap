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

#[derive(Debug, Clone, Copy)]
pub enum KeyType {
    Normal,
    Modifier(u8),
    Lock(u8),
}

/// Compact representation of a key action.
#[derive(Debug, Clone)]
pub struct Key {
    pub scan_code: u16,
    pub layer: u8,
    pub action: KeyAction,
    pub key_type: KeyType,
}

#[derive(Debug, Clone, Default)]
pub struct LayoutBuilder {
    pub(crate) keys: Vec<Key>,
    pub(crate) layer_names: Vec<String>,
}

impl LayoutBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_or_get_layer(&mut self, layer: &str) -> u8 {
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
        self.keys.push(Key {
            scan_code,
            layer,
            action,
            key_type: KeyType::Normal,
        });
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
            action: vk.map_or(KeyAction::Ignore, |vk| KeyAction::VirtualKey(vk)),
            key_type: KeyType::Modifier(target_layer)
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
            action: vk.map_or(KeyAction::Ignore, |vk| KeyAction::VirtualKey(vk)),
            key_type: KeyType::Lock(target_layer)
        };
        self.keys.push(key);
        self
    }
}
