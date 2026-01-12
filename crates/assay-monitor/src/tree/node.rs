use std::collections::HashSet;
use std::fs;
use assay_common::exports::{ProcessNodeExport, ProcessStateExport};

#[derive(Debug, Clone)]
pub struct ProcessNode {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub children: HashSet<u32>,
    pub exe: Option<String>,
    pub cmdline: Option<String>,
    pub cwd: Option<String>,
    pub state: ProcessState,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Exited,
    Killed,
}

impl ProcessNode {
    pub fn new_root(pid: u32) -> Self {
        Self {
            pid,
            parent_pid: None,
            children: HashSet::new(),
            exe: None,
            cmdline: None,
            cwd: None,
            state: ProcessState::Running,
            depth: 0,
        }
    }

    pub fn new_child(pid: u32, parent_pid: u32, parent_depth: u32) -> Self {
        Self {
            pid,
            parent_pid: Some(parent_pid),
            children: HashSet::new(),
            exe: None,
            cmdline: None,
            cwd: None,
            state: ProcessState::Running,
            depth: parent_depth + 1,
        }
    }

    pub fn refresh_metadata(&mut self) {
        if self.state != ProcessState::Running {
            return;
        }
        self.cmdline = read_cmdline(self.pid).ok();
        self.exe = std::fs::read_link(format!("/proc/{}/exe", self.pid))
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
        self.cwd = std::fs::read_link(format!("/proc/{}/cwd", self.pid))
            .ok()
            .map(|p| p.to_string_lossy().into_owned());
    }
}

pub fn read_cmdline(pid: u32) -> std::io::Result<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let content = fs::read_to_string(path)?;
    // cmdline is null-separated, join with spaces
    Ok(content.replace('\0', " ").trim().to_string())
}

/// Reads children of a PID by scanning /proc
/// Note: This is expensive and racy. Only used for initial scan.
pub fn read_children(ppid: u32) -> std::io::Result<Vec<u32>> {
    let mut children = Vec::new();
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let name = entry.file_name();
        let s = name.to_string_lossy();
        if let Ok(pid) = s.parse::<u32>() {
            if let Ok(stat) = fs::read_to_string(format!("/proc/{}/stat", pid)) {
                // PPID is the 4th field in /proc/[pid]/stat
                // Comm can contain spaces and parenthesis, tricky parsing
                // Robust way: find last ')' then 2nd field after
                if let Some(end_comm) = stat.rfind(')') {
                     let rest = &stat[end_comm+1..];
                     let mut parts = rest.split_whitespace();
                     // State is parts[0], PPID is parts[1]
                     if let Some(ppid_str) = parts.nth(1) {
                         if let Ok(p) = ppid_str.parse::<u32>() {
                             if p == ppid {
                                 children.push(pid);
                             }
                         }
                     }
                }
            }
        }
    }
    Ok(children)
}

impl From<&ProcessNode> for ProcessNodeExport {
    fn from(node: &ProcessNode) -> Self {
        Self {
            pid: node.pid,
            parent_pid: node.parent_pid,
            children: node.children.iter().copied().collect(),
            exe: node.exe.clone(),
            cmdline: node.cmdline.clone(),
            cwd: node.cwd.clone(),
            state: match node.state {
                ProcessState::Running => ProcessStateExport::Running,
                ProcessState::Exited => ProcessStateExport::Exited,
                ProcessState::Killed => ProcessStateExport::Killed,
            },
            depth: node.depth,
        }
    }
}
