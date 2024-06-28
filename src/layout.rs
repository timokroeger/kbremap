use std::collections::{HashMap, HashSet};

use petgraph::graph::NodeIndex;
use petgraph::{Directed, Graph};

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
pub type LayerGraph = Graph<String, Vec<ScanCode>, Directed, u8>;
pub type LayerIdx = NodeIndex<u8>;

#[derive(Debug, Clone)]
pub struct Layout {
    /// Key action for all keys including modifiers and locks.
    pub(crate) keymap: HashMap<(LayerIdx, ScanCode), KeyAction>,

    /// Map of keys that lock a specific layer when pressed.
    pub(crate) locks: HashMap<(LayerIdx, ScanCode), LayerIdx>,

    /// Set of scan codes used for layer switching.
    pub(crate) modifier_scan_codes: HashSet<ScanCode>,

    /// Keyboard layout encoded as graph where layers are the nodes and
    /// modifiers (layer change keys) are the egdes.
    pub(crate) layer_graph: LayerGraph,

    /// Active layer when no modifier is pressed.
    pub(crate) base_layer: LayerIdx,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            keymap: HashMap::new(),
            locks: HashMap::new(),
            modifier_scan_codes: HashSet::new(),
            layer_graph: LayerGraph::default(),
            base_layer: LayerIdx::end(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.base_layer != LayerIdx::end()
    }

    pub fn add_layer(&mut self, name: String) -> LayerIdx {
        let layer_idx = self.layer_graph.add_node(name);
        assert!(layer_idx != LayerIdx::end(), "to many layers");
        layer_idx
    }

    pub fn set_base_layer(&mut self, layer: LayerIdx) {
        self.base_layer = layer;
    }

    pub fn add_key(&mut self, scan_code: ScanCode, layer: LayerIdx, action: KeyAction) {
        self.keymap.insert((layer, scan_code), action);
    }

    fn add_edge_scan_code(&mut self, scan_code: ScanCode, layer: LayerIdx, target_layer: LayerIdx) {
        let edge_idx = self
            .layer_graph
            .find_edge(layer, target_layer)
            .unwrap_or_else(|| self.layer_graph.add_edge(layer, target_layer, Vec::new()));
        self.layer_graph[edge_idx].push(scan_code);
    }

    pub fn add_modifier(&mut self, scan_code: ScanCode, layer: LayerIdx, target_layer: LayerIdx) {
        // Add modifiers as edges to the graph.
        self.modifier_scan_codes.insert(scan_code);
        self.add_edge_scan_code(scan_code, layer, target_layer);
    }

    pub fn add_layer_lock(&mut self, scan_code: ScanCode, layer: LayerIdx, target_layer: LayerIdx) {
        self.locks.insert((layer, scan_code), target_layer);

        // Treat locks as modifier so that they change to the target layer on key press
        // (before the actual locking happens on key release).
        self.modifier_scan_codes.insert(scan_code);

        // A lock modifier key can lock the layer it is defined on by targeting the own layer.
        // Skip adding those self-lock modifier to the layer graph to prevent cycles.
        // They would not have any effect because they do not change the layer.
        if layer != target_layer {
            self.add_edge_scan_code(scan_code, layer, target_layer);
        }
    }
}
