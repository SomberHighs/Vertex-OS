//! The Vertex node graph: immutable, content-addressed nodes with typed edges.
//!
//! Everything in the system is a node. A node's identity is the SHA-256 of its
//! canonical form, so:
//!
//!   * identical content collapses onto one node (deduplication),
//!   * a node can never be modified, only superseded (version chains via
//!     `previous`), and
//!   * an id is verifiable: recomputing the hash of a node reproduces its id.
//!
//! Nodes live in a fixed table of heap pointers, so their addresses are stable
//! for the lifetime of the boot.

use crate::arch::x86_64::cpu;
use crate::mm::heap;
use crate::sync::SyncCell;

use super::sha256::Sha256;

pub const HASH_SIZE: usize = 32;
pub type NodeId = [u8; HASH_SIZE];
pub const ZERO_ID: NodeId = [0; HASH_SIZE];

/// A node's semantic type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum NodeType {
    Blob = 0,
    Text = 1,
    Document = 2,
    Directory = 3,
    User = 4,
    Process = 5,
    Capability = 6,
    Schema = 7,
    Custom = 8,
}

impl NodeType {
    pub fn name(self) -> &'static str {
        match self {
            NodeType::Blob => "blob",
            NodeType::Text => "text",
            NodeType::Document => "document",
            NodeType::Directory => "directory",
            NodeType::User => "user",
            NodeType::Process => "process",
            NodeType::Capability => "capability",
            NodeType::Schema => "schema",
            NodeType::Custom => "custom",
        }
    }

    fn from_tag(tag: u8) -> NodeType {
        match tag {
            1 => NodeType::Text,
            2 => NodeType::Document,
            3 => NodeType::Directory,
            4 => NodeType::User,
            5 => NodeType::Process,
            6 => NodeType::Capability,
            7 => NodeType::Schema,
            8 => NodeType::Custom,
            _ => NodeType::Blob,
        }
    }
}

pub const MAX_LABEL: usize = 32;
pub const MAX_EDGES: usize = 16;
pub const MAX_KEY: usize = 24;
pub const MAX_TEXT: usize = 48;
pub const MAX_PROPS: usize = 6;

/// Edge flags, mirroring `graph/node.h`.
pub const EDGE_STRONG: u32 = 0x0001;
pub const EDGE_ORDERED: u32 = 0x0002;
pub const EDGE_UNIQUE: u32 = 0x0004;

/// A labelled, typed relationship to another node.
#[derive(Clone, Copy)]
pub struct Edge {
    label: [u8; MAX_LABEL],
    label_len: u8,
    pub target: NodeId,
    pub flags: u32,
}

impl Edge {
    /// An empty edge, used to initialise arrays.
    pub const fn empty() -> Self {
        Self {
            label: [0; MAX_LABEL],
            label_len: 0,
            target: ZERO_ID,
            flags: 0,
        }
    }

    pub fn new(label: &str, target: NodeId, flags: u32) -> Option<Self> {
        if label.is_empty() || label.len() > MAX_LABEL {
            return None;
        }
        let mut edge = Self {
            label: [0; MAX_LABEL],
            label_len: label.len() as u8,
            target,
            flags,
        };
        edge.label[..label.len()].copy_from_slice(label.as_bytes());
        Some(edge)
    }

    pub fn label(&self) -> &str {
        core::str::from_utf8(&self.label[..self.label_len as usize]).unwrap_or("?")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    Int,
    Bool,
    Text,
}

impl PropKind {
    fn tag(self) -> u8 {
        match self {
            PropKind::Int => 0,
            PropKind::Bool => 1,
            PropKind::Text => 2,
        }
    }
}

/// A key/value property.
#[derive(Clone, Copy)]
pub struct Prop {
    key: [u8; MAX_KEY],
    key_len: u8,
    pub kind: PropKind,
    pub number: i64,
    text: [u8; MAX_TEXT],
    text_len: u8,
}

impl Prop {
    /// An empty property, used to initialise arrays.
    pub const fn empty() -> Self {
        Self {
            key: [0; MAX_KEY],
            key_len: 0,
            kind: PropKind::Int,
            number: 0,
            text: [0; MAX_TEXT],
            text_len: 0,
        }
    }

    pub fn int(key: &str, value: i64) -> Option<Self> {
        Self::blank(key).map(|mut prop| {
            prop.kind = PropKind::Int;
            prop.number = value;
            prop
        })
    }

    pub fn boolean(key: &str, value: bool) -> Option<Self> {
        Self::blank(key).map(|mut prop| {
            prop.kind = PropKind::Bool;
            prop.number = value as i64;
            prop
        })
    }

    pub fn text(key: &str, value: &str) -> Option<Self> {
        Self::blank(key).map(|mut prop| {
            prop.kind = PropKind::Text;
            let len = value.len().min(MAX_TEXT);
            prop.text[..len].copy_from_slice(&value.as_bytes()[..len]);
            prop.text_len = len as u8;
            prop
        })
    }

    fn blank(key: &str) -> Option<Self> {
        if key.is_empty() || key.len() > MAX_KEY {
            return None;
        }
        let mut prop = Self {
            key: [0; MAX_KEY],
            key_len: key.len() as u8,
            kind: PropKind::Int,
            number: 0,
            text: [0; MAX_TEXT],
            text_len: 0,
        };
        prop.key[..key.len()].copy_from_slice(key.as_bytes());
        Some(prop)
    }

    pub fn key(&self) -> &str {
        core::str::from_utf8(&self.key[..self.key_len as usize]).unwrap_or("?")
    }

    pub fn text_value(&self) -> &str {
        core::str::from_utf8(&self.text[..self.text_len as usize]).unwrap_or("")
    }
}

/// An immutable node.
pub struct Node {
    pub id: NodeId,
    pub kind: NodeType,
    pub created: u64,
    pub previous: Option<NodeId>,
    pub data: *const u8,
    pub data_len: usize,
    edges: [Edge; MAX_EDGES],
    edge_count: usize,
    props: [Prop; MAX_PROPS],
    prop_count: usize,
}

impl Node {
    pub fn data(&self) -> &'static [u8] {
        if self.data.is_null() || self.data_len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(self.data, self.data_len) }
        }
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges[..self.edge_count]
    }

    pub fn props(&self) -> &[Prop] {
        &self.props[..self.prop_count]
    }

    pub fn is_directory(&self) -> bool {
        self.kind == NodeType::Directory
    }

    /// Look up a child edge by label.
    pub fn child(&self, label: &str) -> Option<NodeId> {
        self.edges()
            .iter()
            .find(|edge| edge.label() == label)
            .map(|edge| edge.target)
    }

    pub fn set_child_edge(
        &self,
        label: &str,
        target: NodeId,
    ) -> Option<([Edge; MAX_EDGES], usize)> {
        let mut edges = [Edge::empty(); MAX_EDGES];
        let mut count = 0;
        let mut replaced = false;

        for edge in self.edges() {
            if edge.label() == label {
                edges[count] = Edge::new(label, target, EDGE_STRONG | EDGE_UNIQUE)?;
                replaced = true;
            } else {
                edges[count] = *edge;
            }
            count += 1;
        }

        if !replaced {
            if count >= MAX_EDGES {
                return None;
            }
            edges[count] = Edge::new(label, target, EDGE_STRONG | EDGE_UNIQUE)?;
            count += 1;
        }

        sort_edges(&mut edges[..count]);
        Some((edges, count))
    }

    pub fn without_child_edge(&self, label: &str) -> Option<([Edge; MAX_EDGES], usize)> {
        let mut edges = [Edge::empty(); MAX_EDGES];
        let mut count = 0;
        for edge in self.edges() {
            if edge.label() != label {
                edges[count] = *edge;
                count += 1;
            }
        }
        Some((edges, count))
    }
}

fn sort_edges(edges: &mut [Edge]) {
    // Insertion sort: edge lists are tiny and this keeps hashing canonical.
    for i in 1..edges.len() {
        let mut j = i;
        while j > 0 && edges[j - 1].label() > edges[j].label() {
            edges.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Canonical serialisation, hashed to produce the node id.
fn compute_id(
    kind: NodeType,
    data: &[u8],
    previous: Option<NodeId>,
    edges: &[Edge],
    props: &[Prop],
) -> NodeId {
    let mut hasher = Sha256::new();
    hasher.update(&[kind as u8]);
    hasher.update(&(data.len() as u64).to_le_bytes());
    hasher.update(data);
    match previous {
        Some(id) => hasher.update(&id),
        None => hasher.update(&ZERO_ID),
    }

    hasher.update(&[edges.len() as u8]);
    for edge in edges {
        hasher.update(&[edge.label_len]);
        hasher.update(&edge.label[..edge.label_len as usize]);
        hasher.update(&edge.target);
        hasher.update(&edge.flags.to_le_bytes());
    }

    hasher.update(&[props.len() as u8]);
    for prop in props {
        hasher.update(&[prop.key_len]);
        hasher.update(&prop.key[..prop.key_len as usize]);
        hasher.update(&[prop.kind.tag()]);
        hasher.update(&prop.number.to_le_bytes());
        hasher.update(&[prop.text_len]);
        hasher.update(&prop.text[..prop.text_len as usize]);
    }

    hasher.finish()
}

const MAX_NODES: usize = 1024;

struct Store {
    slots: [Option<*mut Node>; MAX_NODES],
    count: usize,
    data_bytes: u64,
    root: NodeId,
    next_sequence: u64,
    initialized: bool,
}

impl Store {
    const fn new() -> Self {
        const NONE: Option<*mut Node> = None;
        Self {
            slots: [NONE; MAX_NODES],
            count: 0,
            data_bytes: 0,
            root: ZERO_ID,
            next_sequence: 1,
            initialized: false,
        }
    }

    fn find(&self, id: &NodeId) -> Option<&'static Node> {
        for slot in self.slots.iter().flatten() {
            let node = unsafe { &**slot };
            if &node.id == id {
                return Some(node);
            }
        }
        None
    }

    fn insert(&mut self, node: Node) -> Option<NodeId> {
        if self.count >= MAX_NODES {
            return None;
        }
        let boxed = unsafe { heap::alloc(core::mem::size_of::<Node>(), 8) as *mut Node };
        if boxed.is_null() {
            return None;
        }
        let id = node.id;
        self.data_bytes += node.data_len as u64;
        unsafe { boxed.write(node) };

        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(boxed);
                self.count += 1;
                return Some(id);
            }
        }
        None
    }
}

static STORE: SyncCell<Store> = SyncCell::new(Store::new());

pub fn init() {
    cpu::without_interrupts(|| unsafe {
        STORE.get().initialized = true;
    });
}

pub fn is_initialized() -> bool {
    cpu::without_interrupts(|| unsafe { STORE.get().initialized })
}

/// Create (or deduplicate) a node and return its content address.
pub fn create(
    kind: NodeType,
    data: &[u8],
    previous: Option<NodeId>,
    edges: &[Edge],
    props: &[Prop],
) -> Option<NodeId> {
    cpu::without_interrupts(|| unsafe {
        let store = STORE.get();
        if !store.initialized {
            return None;
        }

        let mut sorted_edges = [Edge::empty(); MAX_EDGES];
        let edge_count = edges.len().min(MAX_EDGES);
        sorted_edges[..edge_count].copy_from_slice(&edges[..edge_count]);
        sort_edges(&mut sorted_edges[..edge_count]);

        let mut sorted_props = [Prop::empty(); MAX_PROPS];
        let prop_count = props.len().min(MAX_PROPS);
        sorted_props[..prop_count].copy_from_slice(&props[..prop_count]);
        for i in 1..prop_count {
            let mut j = i;
            while j > 0 && sorted_props[j - 1].key() > sorted_props[j].key() {
                sorted_props.swap(j - 1, j);
                j -= 1;
            }
        }

        let id = compute_id(kind, data, previous, &sorted_edges[..edge_count], &sorted_props[..prop_count]);

        // Content addressing: identical content is the same node.
        if store.find(&id).is_some() {
            return Some(id);
        }

        let payload = if data.is_empty() {
            core::ptr::null()
        } else {
            let copy = heap::alloc_copy(data);
            if copy.is_null() {
                return None;
            }
            copy as *const u8
        };

        let sequence = store.next_sequence;
        store.next_sequence += 1;

        let mut node = Node {
            id,
            kind,
            created: sequence,
            previous,
            data: payload,
            data_len: data.len(),
            edges: [Edge::empty(); MAX_EDGES],
            edge_count: 0,
            props: [Prop::empty(); MAX_PROPS],
            prop_count: 0,
        };
        node.edges[..edge_count].copy_from_slice(&sorted_edges[..edge_count]);
        node.edge_count = edge_count;
        node.props[..prop_count].copy_from_slice(&sorted_props[..prop_count]);
        node.prop_count = prop_count;

        store.insert(node)
    })
}

/// Create a directory node from a set of child edges.
pub fn create_directory(edges: &[Edge], previous: Option<NodeId>) -> Option<NodeId> {
    create(NodeType::Directory, &[], previous, edges, &[])
}

pub fn get(id: &NodeId) -> Option<&'static Node> {
    cpu::without_interrupts(|| unsafe { STORE.get().find(id) })
}

pub fn root() -> NodeId {
    cpu::without_interrupts(|| unsafe { STORE.get().root })
}

pub fn set_root(id: NodeId) {
    cpu::without_interrupts(|| unsafe {
        STORE.get().root = id;
    });
}

/// Recompute a node's content address and compare it with its id.
pub fn verify(id: &NodeId) -> bool {
    let Some(node) = get(id) else {
        return false;
    };
    compute_id(node.kind, node.data(), node.previous, node.edges(), node.props()) == *id
}

pub fn node_count() -> usize {
    cpu::without_interrupts(|| unsafe { STORE.get().count })
}

pub fn data_bytes() -> u64 {
    cpu::without_interrupts(|| unsafe { STORE.get().data_bytes })
}

pub fn capacity() -> usize {
    MAX_NODES
}

/// Visit every stored node.
pub fn for_each(mut visit: impl FnMut(&'static Node)) {
    cpu::without_interrupts(|| unsafe {
        let store = STORE.get();
        for slot in store.slots.iter().flatten() {
            visit(&**slot);
        }
    });
}

/// Format an id as 64 hex characters (without a trailing NUL).
pub fn format_id(id: &NodeId, out: &mut [u8; 64]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, byte) in id.iter().enumerate() {
        out[i * 2] = HEX[(byte >> 4) as usize];
        out[i * 2 + 1] = HEX[(byte & 0x0F) as usize];
    }
}

/// Parse a full 64-character hex id.
pub fn parse_id(text: &str) -> Option<NodeId> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut id = ZERO_ID;
    for i in 0..32 {
        let hi = (bytes[i * 2] as char).to_digit(16)? as u8;
        let lo = (bytes[i * 2 + 1] as char).to_digit(16)? as u8;
        id[i] = (hi << 4) | lo;
    }
    Some(id)
}

/// Find a node by a hex prefix (at least 4 characters), returning the first
/// match and whether the prefix was ambiguous.
pub fn find_by_prefix(prefix: &str) -> (Option<&'static Node>, bool) {
    let mut out = [0u8; 64];
    let mut found: Option<&'static Node> = None;
    let mut ambiguous = false;

    for_each(|node| {
        format_id(&node.id, &mut out);
        let hex = core::str::from_utf8(&out).unwrap_or("");
        if hex.starts_with(prefix) {
            if found.is_some() {
                ambiguous = true;
            } else {
                found = Some(node);
            }
        }
    });

    (found, ambiguous)
}

/// Type name for a raw tag, used by the shell when printing edges.
pub fn type_from_tag(tag: u8) -> NodeType {
    NodeType::from_tag(tag)
}
