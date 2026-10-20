//! Build outputs are copied to `<artifacts>/<project>/<build id>/`; the last [`KEEP`] builds stay,
//! so the previous build is always there to go back to.

use std::path::{Path, PathBuf};

/// Builds kept per project.
pub const KEEP: usize = 3;

/// Copies the output folders of a build; returns the build folder and the copied files (relative).
pub fn collect(
    project_dir: &Path,
    outputs: &[String],
    artifacts: &Path,
    project: &str,
    build_id: &str,
) -> std::io::Result<(PathBuf, Vec<String>)> {
    let dest = artifacts.join(project).join(build_id);
    let mut files = Vec::new();
    for out in outputs {
        let src = project_dir.join(out);
        if src.is_dir() {
            copy(&src, &dest.join(out), &mut files, &dest)?;
        }
    }
    if files.is_empty() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    prune(&artifacts.join(project), KEEP)?;
    Ok((dest, files))
}

fn copy(src: &Path, dst: &Path, files: &mut Vec<String>, root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let meta = e.path().symlink_metadata()?;
        let target = dst.join(e.file_name());
        if meta.is_dir() {
            copy(&e.path(), &target, files, root)?;
        } else if meta.is_file() {
            std::fs::copy(e.path(), &target)?;
            files.push(
                target
                    .strip_prefix(root)
                    .unwrap_or(&target)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

/// Builds of a project, newest first.
pub fn list(project_artifacts: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(project_artifacts)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    // Build ids start with a sortable UTC time.
    dirs.sort();
    dirs.reverse();
    dirs
}

fn prune(project_artifacts: &Path, keep: usize) -> std::io::Result<()> {
    for old in list(project_artifacts).into_iter().skip(keep) {
        std::fs::remove_dir_all(old)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_and_keeps_the_last_builds() {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "vf-artifacts-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let project = base.join("p");
        assert!(std::fs::create_dir_all(project.join("build/libs")).is_ok());
        assert!(std::fs::write(project.join("build/libs/app.jar"), "jar").is_ok());
        let outputs = vec!["build/libs".to_owned(), "build/distributions".to_owned()];
        for id in [
            "20261020T010000Z-a",
            "20261020T020000Z-b",
            "20261020T030000Z-c",
            "20261020T040000Z-d",
        ] {
            let r = collect(&project, &outputs, &base.join("artifacts"), "p", id);
            assert_eq!(
                r.ok().map(|(_, f)| f),
                Some(vec!["build/libs/app.jar".to_owned()])
            );
        }
        let kept: Vec<String> = list(&base.join("artifacts/p"))
            .iter()
            .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
            .collect();
        assert_eq!(
            kept,
            [
                "20261020T040000Z-d",
                "20261020T030000Z-c",
                "20261020T020000Z-b"
            ]
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
