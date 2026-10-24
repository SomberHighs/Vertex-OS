//! The Vertex VFS: paths over an immutable content-addressed graph.
//!
//! Directories are nodes whose edges are their entries. Because nodes never
//! change, updating a file means building a new file node and then rebuilding
//! every directory on the path, exactly like a Git tree: the root id is the
//! single pointer that defines the whole filesystem state.

pub mod node;
pub mod sha256;

use core::fmt;

pub use node::{Edge, Node, NodeId, NodeType, Prop, EDGE_STRONG, EDGE_UNIQUE, ZERO_ID};

/// Maximum path depth.
pub const MAX_PATH_SEGMENTS: usize = 8;
/// Maximum length of a single path component.
pub const MAX_NAME: usize = node::MAX_LABEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    NotFound,
    NotADirectory,
    IsDirectory,
    AlreadyExists,
    StoreFull,
    InvalidPath,
    ReadOnly,
}

impl VfsError {
    pub fn message(self) -> &'static str {
        match self {
            VfsError::NotFound => "no such node",
            VfsError::NotADirectory => "not a directory",
            VfsError::IsDirectory => "is a directory",
            VfsError::AlreadyExists => "already exists",
            VfsError::StoreFull => "node store full",
            VfsError::InvalidPath => "invalid path",
            VfsError::ReadOnly => "read-only node",
        }
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

/// Split an absolute or relative path into components.
///
/// `.` segments are dropped; `..` is rejected (the shell resolves it before
/// calling in, because parent links are not part of the graph).
fn split(path: &str) -> Result<([&str; MAX_PATH_SEGMENTS], usize), VfsError> {
    let mut parts = [""; MAX_PATH_SEGMENTS];
    let mut count = 0;
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(VfsError::InvalidPath);
        }
        if segment.len() > MAX_NAME {
            return Err(VfsError::InvalidPath);
        }
        if count == MAX_PATH_SEGMENTS {
            return Err(VfsError::InvalidPath);
        }
        parts[count] = segment;
        count += 1;
    }
    Ok((parts, count))
}

/// Resolve a path against the current root.
pub fn resolve(path: &str) -> Result<NodeId, VfsError> {
    resolve_from(node::root(), path)
}

/// Resolve a path against an explicit starting directory.
pub fn resolve_from(base: NodeId, path: &str) -> Result<NodeId, VfsError> {
    let (parts, count) = split(path)?;
    let mut current = base;

    for segment in &parts[..count] {
        let dir = node::get(&current).ok_or(VfsError::NotFound)?;
        if !dir.is_directory() {
            return Err(VfsError::NotADirectory);
        }
        current = dir.child(segment).ok_or(VfsError::NotFound)?;
    }

    Ok(current)
}

pub fn get(id: &NodeId) -> Option<&'static Node> {
    node::get(id)
}

/// Apply a copy-on-write update to `segs`, rebuilding ancestors.
fn rebuild(
    dir_id: NodeId,
    segments: &[&str],
    op: &mut impl FnMut(NodeId, &str) -> Result<NodeId, VfsError>,
) -> Result<NodeId, VfsError> {
    let dir = node::get(&dir_id).ok_or(VfsError::NotFound)?;
    if !dir.is_directory() {
        return Err(VfsError::NotADirectory);
    }

    let name = segments[0];
    let new_child = if segments.len() == 1 {
        op(dir_id, name)?
    } else {
        let child = dir.child(name).ok_or(VfsError::NotFound)?;
        rebuild(child, &segments[1..], op)?
    };

    let (edges, count) = dir
        .set_child_edge(name, new_child)
        .ok_or(VfsError::StoreFull)?;
    node::create_directory(&edges[..count], Some(dir_id)).ok_or(VfsError::StoreFull)
}

/// Create or replace a file. Returns the new file node id.
pub fn write_file(path: &str, data: &[u8]) -> Result<NodeId, VfsError> {
    let (parts, count) = split(path)?;
    if count == 0 {
        return Err(VfsError::IsDirectory);
    }

    let root = node::root();
    let mut created = ZERO_ID;

    let new_root = rebuild(root, &parts[..count], &mut |dir_id, name| {
        let dir = node::get(&dir_id).ok_or(VfsError::NotFound)?;
        let previous = dir.child(name);
        if let Some(existing) = previous {
            let node = node::get(&existing).ok_or(VfsError::NotFound)?;
            if node.is_directory() {
                return Err(VfsError::IsDirectory);
            }
        }
        // Immutability: a new node supersedes the old one.
        let id = node::create(NodeType::Text, data, previous, &[], &[])
            .ok_or(VfsError::StoreFull)?;
        created = id;
        Ok(id)
    })?;

    node::set_root(new_root);
    Ok(created)
}

/// Create a directory.
pub fn mkdir(path: &str) -> Result<NodeId, VfsError> {
    let (parts, count) = split(path)?;
    if count == 0 {
        return Err(VfsError::AlreadyExists);
    }

    let root = node::root();
    let mut created = ZERO_ID;

    let new_root = rebuild(root, &parts[..count], &mut |dir_id, name| {
        let dir = node::get(&dir_id).ok_or(VfsError::NotFound)?;
        if dir.child(name).is_some() {
            return Err(VfsError::AlreadyExists);
        }
        let id = node::create_directory(&[], None).ok_or(VfsError::StoreFull)?;
        created = id;
        Ok(id)
    })?;

    node::set_root(new_root);
    Ok(created)
}

/// Remove a directory entry, rebuilding the ancestors.
pub fn remove(path: &str) -> Result<NodeId, VfsError> {
    let (parts, count) = split(path)?;
    if count == 0 {
        return Err(VfsError::ReadOnly);
    }

    fn walk(dir_id: NodeId, segments: &[&str]) -> Result<NodeId, VfsError> {
        let dir = node::get(&dir_id).ok_or(VfsError::NotFound)?;
        if !dir.is_directory() {
            return Err(VfsError::NotADirectory);
        }
        let name = segments[0];

        let new_edges = if segments.len() == 1 {
            let child = dir.child(name).ok_or(VfsError::NotFound)?;
            // Refuse to delete a non-empty directory.
            let child_node = node::get(&child).ok_or(VfsError::NotFound)?;
            if child_node.is_directory() && !child_node.edges().is_empty() {
                return Err(VfsError::ReadOnly);
            }
            dir.without_child_edge(name).ok_or(VfsError::StoreFull)?
        } else {
            let child = dir.child(name).ok_or(VfsError::NotFound)?;
            let new_child = walk(child, &segments[1..])?;
            dir.set_child_edge(name, new_child).ok_or(VfsError::StoreFull)?
        };

        node::create_directory(&new_edges.0[..new_edges.1], Some(dir_id))
            .ok_or(VfsError::StoreFull)
    }

    let new_root = walk(node::root(), &parts[..count])?;
    node::set_root(new_root);
    Ok(new_root)
}

/// Number of versions reachable from a node through `previous`.
pub fn version_count(mut id: NodeId) -> usize {
    let mut count = 0;
    while let Some(node) = node::get(&id) {
        count += 1;
        match node.previous {
            Some(previous) => id = previous,
            None => break,
        }
    }
    count
}

const README: &str = "\
Vertex OS 0.2: everything is a node.

This filesystem is a content-addressed graph:
  - nodes are immutable and addressed by SHA-256 of their content
  - identical content is stored exactly once
  - typed edges form directories and relationships
  - 'history' shows the version chain of any path

Try: help, ls /, cat /readme.txt, history /readme.txt
";

const PHILOSOPHY: &str = "\
Unix says everything is a file.
NT says everything is an object.
Vertex says everything is a node.

A node has an identity you can verify, a type you can query,
edges you can traverse, and a history you can walk.
";

/// Seed the initial graph.
pub fn init() {
    node::init();

    let readme = node::create(NodeType::Text, README.as_bytes(), None, &[], &[])
        .expect("seed: readme");
    let philosophy = node::create(NodeType::Text, PHILOSOPHY.as_bytes(), None, &[], &[])
        .expect("seed: philosophy");

    let docs_edges = [Edge::new("philosophy.txt", philosophy, EDGE_STRONG | EDGE_UNIQUE)
        .expect("seed: edge")];
    let docs = node::create_directory(&docs_edges, None).expect("seed: docs");

    let user_props = [
        Prop::text("username", "alice").expect("seed: prop"),
        Prop::text("email", "alice@vertex.os").expect("seed: prop"),
        Prop::boolean("active", true).expect("seed: prop"),
    ];
    let alice = node::create(NodeType::User, &[], None, &[], &user_props).expect("seed: alice");
    let users_edges = [Edge::new("alice", alice, EDGE_STRONG | EDGE_UNIQUE).expect("seed: edge")];
    let users = node::create_directory(&users_edges, None).expect("seed: users");

    let root_edges = [
        Edge::new("users", users, EDGE_STRONG | EDGE_UNIQUE).expect("seed: edge"),
        Edge::new("docs", docs, EDGE_STRONG | EDGE_UNIQUE).expect("seed: edge"),
        Edge::new("readme.txt", readme, EDGE_STRONG | EDGE_UNIQUE).expect("seed: edge"),
    ];
    let root = node::create_directory(&root_edges, None).expect("seed: root");
    node::set_root(root);
}
