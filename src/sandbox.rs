//! Root sandbox for MCP and the server (spec/mcp.md#root-sandbox).

use crate::project::Error;
use std::path::{Component, Path, PathBuf};

fn outside(rel: &str) -> Error {
    Error::new("path_outside_root", format!("'{rel}' is outside the root"))
}

/// Resolve `rel` inside `root` (canonical). Rejects absolute paths, `..` escapes and symlink escapes.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let mut parts = vec![];
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(s) => parts.push(s),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop().ok_or_else(|| outside(rel))?;
            }
            Component::RootDir | Component::Prefix(_) => return Err(outside(rel)),
        }
    }
    let path: PathBuf = parts.iter().fold(root.to_path_buf(), |p, s| p.join(s));
    // The deepest existing ancestor (or the path itself) must stay inside the root after resolving symlinks.
    let mut probe = path.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            if !real.starts_with(root) {
                return Err(outside(rel));
            }
            break;
        }
        probe = probe.parent().ok_or_else(|| outside(rel))?;
    }
    Ok(path)
}

/// Resolve a project path: inside the root and ending in `.slopc.json`.
pub fn project(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    if !rel.ends_with(".slopc.json") {
        return Err(Error::new("bad_path", format!("'{rel}' must end in .slopc.json")));
    }
    resolve(root, rel)
}

/// Project files under `root`, relative and sorted; skips hidden directories, node_modules and target.
pub fn list_projects(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !name.starts_with('.') && name != "node_modules" && name != "target" {
                    walk(root, &e.path(), out);
                }
            } else if name.ends_with(".slopc.json") {
                out.push(e.path().strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = vec![];
    walk(root, root, &mut out);
    out.sort();
    out
}
