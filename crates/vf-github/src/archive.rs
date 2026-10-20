//! Unpacking a GitHub `.tar.gz` into a project folder. The archive is untrusted data: entries are
//! taken only as regular files and folders with safe relative paths; links and special files are
//! skipped; the top folder GitHub adds (`owner-repo-sha/`) is removed.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use crate::GithubError;

/// Most bytes unpacked (500 MB).
const MAX_TOTAL: u64 = 500 * 1024 * 1024;

/// Unpacks `targz` into `dest` (which must not exist). Returns the number of files.
pub fn extract(targz: &[u8], dest: &Path) -> Result<usize, GithubError> {
    if dest.exists() {
        return Err(GithubError::Data(format!(
            "{} already exists",
            dest.display()
        )));
    }
    std::fs::create_dir_all(dest).map_err(|e| GithubError::Data(e.to_string()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(targz));
    let (mut files, mut total) = (0usize, 0u64);
    for entry in archive
        .entries()
        .map_err(|e| GithubError::Data(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| GithubError::Data(e.to_string()))?;
        let kind = entry.header().entry_type();
        let raw = entry
            .path()
            .map_err(|e| GithubError::Data(e.to_string()))?
            .into_owned();
        let Some(rel) = safe_relative(&raw) else {
            continue;
        };
        let target = dest.join(&rel);
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| GithubError::Data(e.to_string()))?;
        } else if kind.is_file() {
            total += entry.header().size().unwrap_or(0);
            if total > MAX_TOTAL {
                return Err(GithubError::Data("the project is too large".into()));
            }
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| GithubError::Data(e.to_string()))?;
            }
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| GithubError::Data(e.to_string()))?;
            std::fs::write(&target, &buf).map_err(|e| GithubError::Data(e.to_string()))?;
            #[cfg(unix)]
            if entry.header().mode().is_ok_and(|m| m & 0o111 != 0) {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
            }
            files += 1;
        }
    }
    Ok(files)
}

/// The path without GitHub's top folder; `None` for anything that is not a plain relative path.
fn safe_relative(raw: &Path) -> Option<PathBuf> {
    let mut parts = raw.components();
    match parts.next()? {
        Component::Normal(_) => {}
        _ => return None,
    }
    let mut out = PathBuf::new();
    for c in parts {
        match c {
            Component::Normal(p) if !p.to_string_lossy().contains(':') => out.push(p),
            _ => return None,
        }
    }
    let first = out.components().next()?;
    // The archive must not bring its own git data.
    if first.as_os_str().eq_ignore_ascii_case(".git") {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (path, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            // Paths are set raw so unsafe ones reach the reader.
            let name = h.as_old_mut().name.as_mut();
            name[..path.len()].copy_from_slice(path.as_bytes());
            h.set_cksum();
            b.append(&h, *data).unwrap_or_default();
        }
        let tar = b.into_inner().unwrap_or_default();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap_or_default();
        gz.finish().unwrap_or_default()
    }

    #[test]
    fn top_folder_removed_and_unsafe_paths_skipped() {
        let dir = std::env::temp_dir().join(format!("vf-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = targz(&[
            ("o-r-123/README.md", b"hi"),
            ("o-r-123/src/a.js", b"x"),
            ("o-r-123/../evil", b"x"),
            ("o-r-123/.git/config", b"x"),
            ("/abs", b"x"),
        ]);
        assert_eq!(extract(&data, &dir).ok(), Some(2));
        assert!(dir.join("README.md").exists() && dir.join("src/a.js").exists());
        assert!(!dir.join(".git").exists() && !dir.parent().unwrap_or(&dir).join("evil").exists());
        assert!(
            extract(&data, &dir).is_err(),
            "never into an existing folder"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
