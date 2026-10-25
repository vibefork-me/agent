//! The policy of a task: the project and the agent's folders, the system, and the data of the
//! coding CLIs. The rest of the user's home stays closed.

use std::path::{Path, PathBuf};

use vf_core::sandbox::SandboxPolicy;

/// System folders: readable (programs, libraries, certificates, DNS settings).
#[cfg(unix)]
const SYSTEM_READ: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/etc",
    "/opt",
    "/nix",
    "/snap",
    "/proc",
    "/sys",
    "/run",
    "/var/lib",
    "/Library",
    "/System",
    "/Applications",
    "/private/etc",
    "/private/var/db",
    "/private/var/select",
];
/// System folders a process needs to write.
#[cfg(unix)]
const SYSTEM_WRITE: &[&str] = &[
    "/dev",
    "/tmp",
    "/var/tmp",
    "/private/tmp",
    "/private/var/folders",
];

/// Inside the home: data of the coding CLIs only (read and write).
/// Package managers (.npm, .gradle, .m2, etc.) are excluded — redirected into agent caches.
const HOME_WRITE: &[&str] = &[
    ".claude",
    ".claude.json",
    ".codex",
    ".cache/claude",
    ".cache/claude-cli-nodejs",
    ".cache/codex",
    ".local/share/claude",
    ".local/share/codex",
    ".local/state/codex",
];
/// Inside the home: tool settings and installs (read only).
const HOME_READ: &[&str] = &[
    ".gitconfig",
    ".npm-global",
    ".config/git",
    ".nvm",
    ".cargo",
    ".rustup",
    ".sdkman",
    ".pyenv",
    ".volta",
];

/// The policy for work in `project`: it and the agent's `tools` / `caches` are writable.
pub fn task_policy(project: &Path, tools: &Path, caches: &Path) -> SandboxPolicy {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let mut read: Vec<PathBuf> = Vec::new();
    let mut write: Vec<PathBuf> = vec![
        project.to_path_buf(),
        tools.to_path_buf(),
        caches.to_path_buf(),
    ];
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        read.push(parent.to_path_buf());
    }
    #[cfg(unix)]
    {
        read.extend(SYSTEM_READ.iter().map(PathBuf::from));
        write.extend(SYSTEM_WRITE.iter().map(PathBuf::from));
    }
    if let Some(home) = &home {
        write.extend(HOME_WRITE.iter().map(|p| home.join(p)));
        read.extend(HOME_READ.iter().map(|p| home.join(p)));
        // `~/.gitconfig` may include other `~/.gitconfig-*` files.
        if let Ok(entries) = std::fs::read_dir(home) {
            read.extend(
                entries
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with(".gitconfig"))
                    .map(|e| e.path()),
            );
        }
    }
    #[cfg(unix)]
    if let Some(tmp) = std::env::var_os("TMPDIR").map(PathBuf::from) {
        write.push(tmp);
    }
    // Windows: CLIs installed per user; a temp folder inside the agent's caches.
    #[cfg(windows)]
    let temp = {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
        if let Some(l) = &local {
            read.push(l.join("OpenAI").join("Codex"));
        }
        if let Some(r) = &roaming {
            read.push(r.join("npm"));
        }
        let t = caches.join("tmp");
        let _ = std::fs::create_dir_all(&t);
        Some(t)
    };
    #[cfg(not(windows))]
    let temp = None;
    // Only paths that exist (a rule for a missing path cannot be created).
    read.retain(|p| p.exists());
    write.retain(|p| p.exists());
    SandboxPolicy { read, write, temp }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_and_agent_folders_are_writable_home_is_not() {
        let tmp = std::env::temp_dir();
        let p = task_policy(&tmp, &tmp, &tmp);
        assert!(p.write.contains(&tmp));
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
        {
            assert!(
                !p.read.contains(&home) && !p.write.contains(&home),
                "the home itself is not open"
            );
            assert!(p.read.iter().chain(&p.write).all(|x| !x.ends_with(".ssh")));

            // Package managers must never be writable in home
            for blocked in [
                ".gradle",
                ".m2",
                ".npm",
                ".cache",
                ".local",
                ".bun",
                ".yarn",
                ".pnpm-store",
            ] {
                assert!(
                    !p.write.contains(&home.join(blocked)),
                    "{blocked} must not be writable in home"
                );
            }
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            assert!(p.read.iter().any(|path| path == parent));
        }
    }
}
