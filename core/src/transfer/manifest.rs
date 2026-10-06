//! Turns user-selected files and folders into an offer manifest.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::{Error, Result};
use crate::pathsafe::{sanitize_component, unique_name};
use crate::proto::{Entry, MAX_ENTRIES};

/// Where the bytes of a manifest entry come from.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Source {
    Dir,
    Path(PathBuf),
    /// Opaque handle resolved by the host app (Android `content://` URI).
    Uri(String),
}

fn mtime_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|n| sanitize_component(&n.to_string_lossy()))
        .ok_or_else(|| Error::InvalidArgument(format!("{} has no file name", path.display())))
}

/// Builds entries for the given paths. Folders are walked recursively and
/// keep their structure, including empty subfolders. Symlinks inside
/// folders are skipped so a link cannot pull in unrelated files or loop.
pub fn build(paths: &[PathBuf]) -> Result<Vec<(Entry, Source)>> {
    let mut out: Vec<(Entry, Source)> = Vec::new();
    let mut roots: HashSet<String> = HashSet::new();

    for path in paths {
        let meta = std::fs::metadata(path).map_err(|e| Error::InvalidArgument(format!("{}: {e}", path.display())))?;
        let name = unique_name(&file_name(path)?, meta.is_dir(), |n| roots.contains(&n.to_lowercase()));
        roots.insert(name.to_lowercase());

        if meta.is_file() {
            out.push((Entry { path: name, is_dir: false, size: meta.len(), mtime: mtime_of(&meta) }, Source::Path(path.clone())));
        } else if meta.is_dir() {
            out.push((Entry { path: name.clone(), is_dir: true, size: 0, mtime: mtime_of(&meta) }, Source::Dir));
            walk(path, &name, &mut out)?;
        }
        if out.len() > MAX_ENTRIES {
            return Err(Error::InvalidArgument(format!("more than {MAX_ENTRIES} items selected")));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<(Entry, Source)>) -> Result<()> {
    // Explicit stack instead of recursion: deep trees must not overflow.
    let mut stack = vec![(dir.to_path_buf(), rel.to_string())];
    while let Some((dir, rel)) = stack.pop() {
        let mut children: Vec<_> = match std::fs::read_dir(&dir) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(e) => {
                tracing::warn!("skipping unreadable folder {}: {e}", dir.display());
                continue;
            }
        };
        children.sort_by_key(|e| e.file_name());

        let mut taken: HashSet<String> = HashSet::new();
        let mut subdirs = Vec::new();
        for child in children {
            let Ok(meta) = std::fs::symlink_metadata(child.path()) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            let name = unique_name(&sanitize_component(&child.file_name().to_string_lossy()), meta.is_dir(), |n| taken.contains(&n.to_lowercase()));
            taken.insert(name.to_lowercase());
            let child_rel = format!("{rel}/{name}");
            if child_rel.split('/').count() > crate::pathsafe::MAX_DEPTH {
                tracing::warn!("skipping {}: folder nesting too deep", child.path().display());
                continue;
            }
            if meta.is_dir() {
                out.push((Entry { path: child_rel.clone(), is_dir: true, size: 0, mtime: mtime_of(&meta) }, Source::Dir));
                subdirs.push((child.path(), child_rel));
            } else if meta.is_file() {
                out.push((Entry { path: child_rel, is_dir: false, size: meta.len(), mtime: mtime_of(&meta) }, Source::Path(child.path())));
            }
            if out.len() > MAX_ENTRIES {
                return Err(Error::InvalidArgument(format!("more than {MAX_ENTRIES} items selected")));
            }
        }
        // Reverse so folders are visited in sorted order.
        stack.extend(subdirs.into_iter().rev());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_folders_with_structure() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Proyek");
        std::fs::create_dir_all(root.join("src/deep")).unwrap();
        std::fs::create_dir_all(root.join("kosong")).unwrap();
        std::fs::write(root.join("src/main.rs"), b"fn main(){}").unwrap();
        std::fs::write(root.join("src/deep/x.bin"), vec![7u8; 1000]).unwrap();
        std::fs::write(d.path().join("single.txt"), b"hi").unwrap();

        let entries = build(&[root.clone(), d.path().join("single.txt")]).unwrap();
        let paths: Vec<_> = entries.iter().map(|(e, _)| (e.path.as_str(), e.is_dir, e.size)).collect();
        assert_eq!(
            paths,
            vec![
                ("Proyek", true, 0),
                ("Proyek/kosong", true, 0),
                ("Proyek/src", true, 0),
                ("Proyek/src/deep", true, 0),
                ("Proyek/src/main.rs", false, 11),
                ("Proyek/src/deep/x.bin", false, 1000),
                ("single.txt", false, 2),
            ]
        );
        for (e, _) in &entries {
            crate::pathsafe::validate(&e.path).unwrap();
        }
    }

    #[test]
    fn duplicate_root_names_are_renamed() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("x.txt"), b"a").unwrap();
        std::fs::write(b.path().join("x.txt"), b"b").unwrap();
        let entries = build(&[a.path().join("x.txt"), b.path().join("x.txt")]).unwrap();
        assert_eq!(entries[0].0.path, "x.txt");
        assert_eq!(entries[1].0.path, "x (1).txt");
    }
}
