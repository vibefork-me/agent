//! Tools of the agent's own coding loop. Every path is a [`ProjectPath`] that must also stay inside
//! the project after links are resolved (invariant 8); `.git` is not reachable, and coding CLI
//! configuration ([`vf_core::CLI_CONFIG`]) is not writable.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};
use vf_core::ProjectPath;
use vf_protocol::Cancel;

use crate::ChildEnv;
use crate::process;

/// Timeout of `run_command`.
pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
/// Folders skipped by `list_dir`.
const SKIPPED: &[&str] = &[".git", "node_modules", "build", ".gradle", "dist", "target"];

/// Tool definitions in the OpenAI `tools` format.
pub(crate) fn definitions() -> Value {
    let f = |name: &str, description: &str, properties: Value, required: &[&str]| {
        json!({ "type": "function", "function": {
            "name": name, "description": description,
            "parameters": { "type": "object", "properties": properties, "required": required }
        }})
    };
    json!([
        f(
            "list_dir",
            "List files and folders at a path inside the project (recursive up to depth 3, skips .git/node_modules/build).",
            json!({ "path": { "type": "string", "description": "Relative path, \".\" for the project root" } }),
            &["path"]
        ),
        f(
            "read_file",
            "Read a text file inside the project.",
            json!({ "path": { "type": "string" } }),
            &["path"]
        ),
        f(
            "write_file",
            "Create or overwrite a text file inside the project with the full new content.",
            json!({ "path": { "type": "string" }, "content": { "type": "string" } }),
            &["path", "content"]
        ),
        f(
            "edit_file",
            "Replace an exact, unique snippet in a file.",
            json!({ "path": { "type": "string" }, "old": { "type": "string" }, "new": { "type": "string" } }),
            &["path", "old", "new"]
        ),
        f(
            "run_command",
            &format!(
                "Run a shell command in the project folder (build, run, test). 3 minute timeout. {}",
                shell_hint()
            ),
            json!({ "command": { "type": "string" } }),
            &["command"]
        ),
        f(
            "finish",
            "Call when the task is complete. Give a short summary for the user.",
            json!({ "summary": { "type": "string" } }),
            &["summary"]
        ),
    ])
}

fn shell_hint() -> &'static str {
    if cfg!(windows) {
        "Windows: cmd.exe syntax."
    } else {
        "POSIX sh syntax."
    }
}

/// Result of one tool call.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Text for the model.
    Done(String),
    /// `finish` was called.
    Finished(String),
}

/// Executes a tool call.
pub(crate) fn call(
    dir: &Path,
    name: &str,
    args: &Value,
    env: &ChildEnv,
    cancel: &Cancel,
) -> Result<Outcome, String> {
    let arg = |k: &str| args[k].as_str().unwrap_or_default();
    match name {
        "list_dir" => list_dir(dir, arg("path")).map(Outcome::Done),
        "read_file" => {
            let p = inside(dir, arg("path"))?;
            let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
            Ok(Outcome::Done(crate::clip(&text, 20_000)))
        }
        "write_file" => {
            let p = writable(dir, arg("path"))?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(p, arg("content")).map_err(|e| e.to_string())?;
            Ok(Outcome::Done("written".into()))
        }
        "edit_file" => {
            let p = writable(dir, arg("path"))?;
            let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
            let old = arg("old");
            let n = if old.is_empty() {
                0
            } else {
                text.matches(old).count()
            };
            if n != 1 {
                return Err(if n == 0 {
                    "snippet not found".into()
                } else {
                    format!("snippet occurs {n} times, must be unique")
                });
            }
            std::fs::write(&p, text.replacen(old, arg("new"), 1)).map_err(|e| e.to_string())?;
            Ok(Outcome::Done("edited".into()))
        }
        "run_command" => run_command(dir, arg("command"), env, cancel).map(Outcome::Done),
        "finish" => Ok(Outcome::Finished(arg("summary").to_owned())),
        other => Err(format!("unknown tool {other}")),
    }
}

/// Resolves a model-given path inside the project.
pub(crate) fn inside(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let raw = if raw.is_empty() { "." } else { raw };
    let rel = ProjectPath::parse(raw).map_err(|e| e.to_string())?;
    if rel
        .as_path()
        .components()
        .next()
        .is_some_and(|c| c.as_os_str().eq_ignore_ascii_case(".git"))
    {
        return Err("the .git folder is not accessible".into());
    }
    let full = rel.under(root);
    // Links: the nearest existing ancestor must resolve inside the project.
    let real_root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut probe = full.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            if !real.starts_with(&real_root) {
                return Err("path outside the project".into());
            }
            break;
        }
        match probe.parent() {
            Some(p) => probe = p,
            None => break,
        }
    }
    Ok(full)
}

/// Resolves a model-given path the model may write: inside the project and not coding CLI
/// configuration, neither as written nor after links and short names are resolved.
pub(crate) fn writable(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let full = inside(root, raw)?;
    let lexical = ProjectPath::parse(if raw.is_empty() { "." } else { raw })
        .map_err(|e| e.to_string())?;
    let real = real_relative(root, &full);
    if lexical.is_cli_config() || real.is_some_and(|r| vf_core::path::is_cli_config(&r)) {
        return Err("coding CLI settings of the project are not writable".into());
    }
    Ok(full)
}

/// `full` relative to the real project root: its nearest existing ancestor resolved, the rest
/// appended as written.
fn real_relative(root: &Path, full: &Path) -> Option<PathBuf> {
    let real_root = root.canonicalize().ok()?;
    let mut probe = full;
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut rel = real.strip_prefix(&real_root).ok()?.to_path_buf();
            rel.extend(rest.iter().rev());
            return Some(rel);
        }
        rest.push(probe.file_name()?.to_owned());
        probe = probe.parent()?;
    }
}

fn list_dir(root: &Path, raw: &str) -> Result<String, String> {
    let start = inside(root, raw)?;
    let mut out = Vec::new();
    walk(root, &start, 1, &mut out);
    Ok(if out.is_empty() {
        "(empty)".into()
    } else {
        out.join("\n")
    })
}

fn walk(root: &Path, dir: &Path, depth: u32, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if SKIPPED.contains(&name.as_str()) {
            continue;
        }
        let path = e.path();
        let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push(if is_dir { format!("{rel}/") } else { rel });
        if is_dir && depth < 3 && out.len() < 400 {
            walk(root, &path, depth + 1, out);
        }
    }
}

fn run_command(
    dir: &Path,
    command: &str,
    env: &ChildEnv,
    cancel: &Cancel,
) -> Result<String, String> {
    let mut cmd = shell(command, env);
    cmd.current_dir(dir);
    let mut text = String::new();
    let exit = process::run_streaming(cmd, None, cancel, Some(COMMAND_TIMEOUT), &mut |_, line| {
        text.push_str(&line);
        text.push('\n');
    })
    .map_err(|e| e.to_string())?;
    if exit.timed_out {
        text.push_str("[timeout after 180s]\n");
    }
    let tail: String = {
        let chars: Vec<char> = text.chars().collect();
        chars[chars.len().saturating_sub(6000)..].iter().collect()
    };
    Ok(format!("exit code {}\n{tail}", exit.code))
}

/// The command through the system shell: `cmd /d /s /c "<command>"` (the line is passed as is,
/// like a terminal would) or `/bin/sh -c <command>`.
#[cfg(windows)]
fn shell(command: &str, env: &ChildEnv) -> Command {
    use std::os::windows::process::CommandExt as _;
    let mut c = env.command(Path::new("cmd.exe"));
    c.arg("/d")
        .arg("/s")
        .arg("/c")
        .raw_arg(format!("\"{command}\""));
    c
}

#[cfg(not(windows))]
fn shell(command: &str, env: &ChildEnv) -> Command {
    let mut c = env.command(Path::new("/bin/sh"));
    c.args(["-c", command]);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir =
            std::env::temp_dir().join(format!("vf-tools-{}-{nanos}-{counter}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join(".git"));
        dir
    }

    #[test]
    fn paths_stay_inside_and_git_is_hidden() {
        let dir = project();
        assert!(inside(&dir, "src/a.txt").is_ok());
        assert!(inside(&dir, ".").is_ok());
        for bad in [
            "../x",
            "/etc/passwd",
            "C:\\Windows\\x",
            ".git/config",
            ".GIT/hooks/pre-commit",
            "a/../../x",
        ] {
            assert!(inside(&dir, bad).is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli_settings_are_not_writable() {
        let dir = project();
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(dir.join(".claude/settings.json"), "{}").unwrap();
        let env = ChildEnv::from_current();
        let c = Cancel::new();
        let run = |name: &str, args: Value| call(&dir, name, &args, &env, &c);
        for path in [
            ".claude/settings.json",
            ".claude/settings.local.json",
            "src/../.CLAUDE/settings.json",
            ".codex/config.toml",
            ".mcp.json",
            ".github/hooks/hooks.json",
        ] {
            assert!(
                run("write_file", json!({"path": path, "content": "{}"})).is_err(),
                "{path}"
            );
        }
        assert!(
            run(
                "edit_file",
                json!({"path": ".claude/settings.json", "old": "{}", "new": "{\"hooks\":{}}"})
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(".claude/settings.json")).unwrap(),
            "{}"
        );
        assert!(!dir.join(".codex").exists() && !dir.join(".mcp.json").exists());
        // Reading stays allowed: the settings are data.
        assert!(run("read_file", json!({"path": ".claude/settings.json"})).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_edit_read_list() {
        let dir = project();
        let env = ChildEnv::from_current();
        let c = Cancel::new();
        let run = |name: &str, args: Value| call(&dir, name, &args, &env, &c);
        assert_eq!(
            run(
                "write_file",
                json!({"path": "src/a.txt", "content": "one two"})
            ),
            Ok(Outcome::Done("written".into()))
        );
        assert!(
            run(
                "edit_file",
                json!({"path": "src/a.txt", "old": "zzz", "new": "x"})
            )
            .is_err()
        );
        assert!(
            run(
                "edit_file",
                json!({"path": "src/a.txt", "old": "two", "new": "three"})
            )
            .is_ok()
        );
        assert_eq!(
            run("read_file", json!({"path": "src/a.txt"})),
            Ok(Outcome::Done("one three".into()))
        );
        assert_eq!(
            run("list_dir", json!({"path": "."})),
            Ok(Outcome::Done("src/\nsrc/a.txt".into()))
        );
        let echoed = run("run_command", json!({"command": "echo hi there"}));
        assert!(
            matches!(&echoed, Ok(Outcome::Done(t)) if t.starts_with("exit code 0") && t.contains("hi there")),
            "{echoed:?}"
        );
        assert_eq!(
            run("finish", json!({"summary": "ok"})),
            Ok(Outcome::Finished("ok".into()))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
