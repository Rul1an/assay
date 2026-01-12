use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{RwLock};
use assay_common::exports::ProcessTreeExport;
use super::node::{ProcessNode, read_children};

/// Configuration for the tracker
#[derive(Debug, Clone)]
pub struct TrackerConfig {
    /// Maximum tree depth to prevent recursion issues in visuals
    pub max_depth: u32,

    /// Scan existing children on root attach
    pub scan_existing_children: bool,

    /// Refresh process metadata (exe, cmdline) on attach
    pub refresh_metadata: bool,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            max_depth: 20,
            scan_existing_children: true,
            refresh_metadata: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrackerError {
    #[error("PID {0} already tracked")]
    AlreadyTracked(u32),

    #[error("Lock poisoned")]
    LockPoisoned,
}

pub type TrackerResult<T> = Result<T, TrackerError>;

/// Thread-safe process tree tracker (Forensics Only).
pub struct ProcessTreeTracker {
    inner: RwLock<TrackerInner>,
    config: TrackerConfig,
}

struct TrackerInner {
    nodes: HashMap<u32, ProcessNode>,
    roots: HashSet<u32>,
}

impl ProcessTreeTracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            inner: RwLock::new(TrackerInner {
                nodes: HashMap::new(),
                roots: HashSet::new(),
            }),
            config,
        }
    }

    pub fn track_root(&self, pid: u32) -> TrackerResult<()> {
        let mut inner = self.inner.write().map_err(|_| TrackerError::LockPoisoned)?;

        if inner.nodes.contains_key(&pid) {
            return Err(TrackerError::AlreadyTracked(pid));
        }

        let mut node = ProcessNode::new_root(pid);
        if self.config.refresh_metadata {
            node.refresh_metadata();
        }

        inner.roots.insert(pid);
        inner.nodes.insert(pid, node);

        if self.config.scan_existing_children {
            drop(inner);
            self.scan_children(pid, 0)?;
        }
        Ok(())
    }

    pub fn on_fork(&self, parent_pid: u32, child_pid: u32) -> TrackerResult<()> {
        let mut inner = self.inner.write().map_err(|_| TrackerError::LockPoisoned)?;

        let parent_depth = match inner.nodes.get(&parent_pid) {
            Some(parent) => parent.depth,
            None => return Ok(()), // Parent not tracked, ignore
        };

        if parent_depth >= self.config.max_depth {
            return Ok(()); // Depth exceeded, ignore for visuals
        }

        if inner.nodes.contains_key(&child_pid) {
            return Ok(());
        }

        let mut child_node = ProcessNode::new_child(child_pid, parent_pid, parent_depth);
        if self.config.refresh_metadata {
            child_node.refresh_metadata();
        }

        if let Some(parent) = inner.nodes.get_mut(&parent_pid) {
            parent.children.insert(child_pid);
        }

        inner.nodes.insert(child_pid, child_node);
        Ok(())
    }

    pub fn on_exit(&self, pid: u32) -> TrackerResult<()> {
        let mut inner = self.inner.write().map_err(|_| TrackerError::LockPoisoned)?;

        // Remove from parent
        let parent_pid = inner.nodes.get(&pid).and_then(|n| n.parent_pid);
        if let Some(ppid) = parent_pid {
            if let Some(parent) = inner.nodes.get_mut(&ppid) {
                parent.children.remove(&pid);
            }
        }

        // We do *not* remove the node entirely if we want forensics history.
        // But for memory management we might want to "mark exited" instead of drop?
        // SOTA: Keep dead nodes for the session report?
        // Prototype removed them. I will follow prototype: Remove from active tree (memory),
        // but typically IncidentBundle is built *during* the event or at the end.
        // If we remove them, we lose the tree structure for the report unless we snapshotted it.
        // Actually, let's keep them but mark state=Exited.

        if let Some(node) = inner.nodes.get_mut(&pid) {
            node.state = super::node::ProcessState::Exited;
        }

        // Remove from roots if it was a root? No, keep logic consistent.
        // If we want to prune memory, we need a separate "gc_dead()"

        Ok(())
    }

    /// Scan existing children (recursive)
    fn scan_children(&self, pid: u32, current_depth: u32) -> TrackerResult<()> {
        if current_depth >= self.config.max_depth {
            return Ok(());
        }

        let children = read_children(pid).unwrap_or_default();
        for child_pid in children {
            if let Err(_) = self.on_fork(pid, child_pid) {
                continue;
            }
            let _ = self.scan_children(child_pid, current_depth + 1);
        }
        Ok(())
    }

    pub fn export(&self) -> TrackerResult<ProcessTreeExport> {
        let inner = self.inner.read().map_err(|_| TrackerError::LockPoisoned)?;
        Ok(ProcessTreeExport {
            roots: inner.roots.iter().copied().collect(),
            nodes: inner.nodes.iter()
                .map(|(&pid, node)| (pid, node.into()))
                .collect(),
            total_count: inner.nodes.len(),
        })
    }
}
