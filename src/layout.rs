use std::collections::HashMap;

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

pub type ScanCode = u16;
pub type LayerIdx = u8;

#[derive(Debug)]
pub struct Layout {
    /// Key action for all keys including modifiers and locks.
    keymap: HashMap<(LayerIdx, ScanCode), KeyAction>,

    /// Map of keys that change layer when pressed.
    modifiers: HashMap<(LayerIdx, ScanCode), LayerIdx>,

    /// Map of keys that lock a specific layer when pressed.
    locks: HashMap<(LayerIdx, ScanCode), LayerIdx>,

    max_layer: LayerIdx,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        Self {
            keymap: HashMap::new(),
            modifiers: HashMap::new(),
            locks: HashMap::new(),
            max_layer: 0,
        }
    }

    pub fn add_layer(&mut self) -> LayerIdx {
        let layer_idx = self.max_layer;
        self.max_layer += 1;
        layer_idx
    }

    pub fn add_key(&mut self, scan_code: ScanCode, layer: LayerIdx, action: KeyAction) {
        self.keymap.insert((layer, scan_code), action);
    }

    pub fn add_modifier(&mut self, scan_code: ScanCode, layer: LayerIdx, target_layer: LayerIdx) {
        self.modifiers.insert((layer, scan_code), target_layer);
    }

    pub fn add_layer_lock(&mut self, scan_code: ScanCode, layer: LayerIdx, target_layer: LayerIdx) {
        self.locks.insert((layer, scan_code), target_layer);
    }

    pub fn action(&self, layer: LayerIdx, scan_code: ScanCode) -> Option<KeyAction> {
        self.keymap.get(&(layer, scan_code)).copied()
    }

    pub fn layer_modifier(&self, layer: LayerIdx, scan_code: ScanCode) -> Option<LayerIdx> {
        self.modifiers.get(&(layer, scan_code)).copied()
    }

    pub fn layer_lock(&self, layer: LayerIdx, scan_code: ScanCode) -> Option<LayerIdx> {
        self.locks.get(&(layer, scan_code)).copied()
    }
}
