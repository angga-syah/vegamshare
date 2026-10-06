//! Validation of relative paths received from a peer.
//!
//! A manifest path is untrusted input: it must never escape the receiver's
//! destination folder, and it must map to exactly one file name on every
//! platform we support (Windows is the strictest). Paths travel as `/`
//! separated components.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_COMPONENT_BYTES: usize = 255;
pub const MAX_DEPTH: usize = 64;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
    "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

fn is_forbidden_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20 || c == '\u{7f}'
}

fn is_reserved_name(component: &str) -> bool {
    // Windows treats "CON.txt" and "con .tar.gz" like "CON".
    let stem = component.split('.').next().unwrap_or("").trim_end_matches(' ');
    WINDOWS_RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

fn check_component(c: &str) -> Result<()> {
    let bad = |why: &str| Err(Error::InvalidPath(format!("{c:?}: {why}")));
    if c.is_empty() {
        return bad("empty component");
    }
    if c == "." || c == ".." {
        return bad("relative component");
    }
    if c.len() > MAX_COMPONENT_BYTES {
        return bad("name too long");
    }
    if c.chars().any(is_forbidden_char) {
        return bad("forbidden character");
    }
    // Windows silently strips these, which would let two names alias.
    if c.ends_with('.') || c.ends_with(' ') {
        return bad("trailing dot or space");
    }
    if is_reserved_name(c) {
        return bad("reserved device name");
    }
    Ok(())
}

/// Validates a peer-supplied relative path and returns its components.
pub fn validate(path: &str) -> Result<Vec<&str>> {
    if path.len() > MAX_PATH_BYTES {
        return Err(Error::InvalidPath("path too long".into()));
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > MAX_DEPTH {
        return Err(Error::InvalidPath("path too deep".into()));
    }
    for p in &parts {
        check_component(p)?;
    }
    Ok(parts)
}

/// Makes a local file name acceptable to [`validate`] so the sender never
/// offers something the receiver would refuse (e.g. `a:b` from Android).
pub fn sanitize_component(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if is_forbidden_char(c) { '_' } else { c }).collect();
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() || out == "." || out == ".." {
        out = "_".into();
    }
    if is_reserved_name(&out) {
        out.insert(0, '_');
    }
    if out.len() > MAX_COMPONENT_BYTES {
        let mut cut = MAX_COMPONENT_BYTES;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

/// Joins validated components under `root`, refusing to traverse any
/// existing symlink so a pre-planted link cannot redirect writes.
pub fn join_under(root: &Path, components: &[&str]) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for c in components {
        out.push(c);
        if let Ok(meta) = std::fs::symlink_metadata(&out)
            && meta.file_type().is_symlink()
        {
            return Err(Error::InvalidPath(format!("{} is a symlink", out.display())));
        }
    }
    Ok(out)
}

/// Returns `name`, or `name (1)`, `name (2)`, ... such that `exists` is false.
/// The counter is inserted before the extension of files (`a (1).txt`).
pub fn unique_name(name: &str, is_dir: bool, exists: impl Fn(&str) -> bool) -> String {
    if !exists(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1u32..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !exists(candidate))
        .expect("u32 range is not exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        for p in ["a.txt", "Photos/2024/IMG 0001.jpg", "folder/sub/.hidden", "日本語/ファイル.txt", ".git"] {
            assert!(validate(p).is_ok(), "{p}");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for p in [
            "../etc/passwd", "a/../../b", "..", ".", "/etc/passwd", "C:\\Windows", "C:/x", "a\\..\\b",
            "a//b", "", "a/", "\\\\server\\share",
        ] {
            assert!(validate(p).is_err(), "{p:?} should be rejected");
        }
    }

    #[test]
    fn rejects_windows_hazards() {
        for p in ["CON", "con.txt", "dir/NUL", "Com1.log", "LPT9", "aux .txt", "a.", "a ", "x/a:b", "a*b", "a\u{0}b", "a\nb"] {
            assert!(validate(p).is_err(), "{p:?} should be rejected");
        }
        assert!(validate("CONSOLE.txt").is_ok());
        assert!(validate("com10").is_ok());
    }

    #[test]
    fn rejects_oversized() {
        assert!(validate(&"a".repeat(256)).is_err());
        assert!(validate(&vec!["d"; MAX_DEPTH + 1].join("/")).is_err());
        assert!(validate(&vec!["d"; MAX_DEPTH].join("/")).is_ok());
    }

    #[test]
    fn sanitized_names_always_validate() {
        for n in ["a:b", "CON", "..", ".", "x. ", "", "what?*", "nul.txt", &"é".repeat(200), "\u{1}"] {
            let s = sanitize_component(n);
            assert!(validate(&s).is_ok(), "{n:?} -> {s:?}");
        }
        assert_eq!(sanitize_component("normal.txt"), "normal.txt");
    }

    #[test]
    fn unique_name_inserts_counter_before_extension() {
        let taken = ["a.txt", "a (1).txt", "dir"];
        let exists = |n: &str| taken.contains(&n);
        assert_eq!(unique_name("b.txt", false, exists), "b.txt");
        assert_eq!(unique_name("a.txt", false, exists), "a (2).txt");
        assert_eq!(unique_name("dir", true, exists), "dir (1)");
        assert_eq!(unique_name(".bashrc", false, |n| n == ".bashrc"), ".bashrc (1)");
    }

    #[cfg(unix)]
    #[test]
    fn join_refuses_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/tmp", dir.path().join("link")).unwrap();
        assert!(join_under(dir.path(), &["link", "x"]).is_err());
        assert!(join_under(dir.path(), &["real", "x"]).is_ok());
    }
}
