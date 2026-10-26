//! Will this machine manage the build? Needs of the project (ecosystem profile, maintainer's hint)
//! against what the machine has.

use std::path::Path;

use crate::recipe::Ecosystem;

const GB: u64 = 1024 * 1024 * 1024;

/// What a build needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    /// Memory, bytes.
    pub memory: u64,
    /// Disk (toolchain, caches, outputs), bytes.
    pub disk: u64,
    /// Rough time on an average machine, minutes.
    pub minutes: u32,
}

/// What the machine has now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// Logical CPUs.
    pub cpus: usize,
    /// Total memory, bytes.
    pub memory_total: u64,
    /// Available memory, bytes.
    pub memory_available: u64,
    /// Free disk where tools, caches and artifacts live, bytes.
    pub disk_free: u64,
}

impl Machine {
    /// Measures this machine; `disk_of` is a folder on the disk that receives the build.
    pub fn measure(disk_of: &Path) -> Self {
        use sysinfo::{Disks, System};
        let mut sys = System::new();
        sys.refresh_memory();
        let disks = Disks::new_with_refreshed_list();
        let target = disk_of
            .canonicalize()
            .unwrap_or_else(|_| disk_of.to_path_buf());
        let disk_free = disks
            .list()
            .iter()
            .filter(|d| {
                target.starts_with(d.mount_point())
                    || strip_verbatim(&target).starts_with(d.mount_point())
            })
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| d.available_space())
            .unwrap_or(0);
        Self {
            cpus: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            memory_total: sys.total_memory(),
            memory_available: sys.available_memory(),
            disk_free,
        }
    }
}

/// `\\?\C:\x` → `C:\x` (canonical paths on Windows).
fn strip_verbatim(p: &Path) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    std::path::PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
}

/// Profile of an ecosystem.
pub fn profile(e: Ecosystem) -> Needs {
    match e {
        Ecosystem::Gradle => Needs {
            memory: 4 * GB,
            disk: 3 * GB,
            minutes: 5,
        },
        Ecosystem::Maven => Needs {
            memory: 2 * GB,
            disk: 2 * GB,
            minutes: 4,
        },
        Ecosystem::Rust => Needs {
            memory: 4 * GB,
            disk: 4 * GB,
            minutes: 10,
        },
        Ecosystem::Go => Needs {
            memory: 2 * GB,
            disk: GB,
            minutes: 2,
        },
        Ecosystem::Node => Needs {
            memory: 2 * GB,
            disk: GB,
            minutes: 2,
        },
        Ecosystem::Python => Needs {
            memory: 2 * GB,
            disk: GB,
            minutes: 2,
        },
    }
}

/// The maintainer's memory hint in `AGENTS.md`: "8 GB RAM", "16GB of memory".
pub fn agents_memory_hint(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')'))
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        // "8gb" or "8 gb".
        let number = match w.strip_suffix("gb") {
            Some(n) if !n.is_empty() => n,
            _ if words.get(i + 1).is_some_and(|n| n.starts_with("gb")) => w,
            _ => continue,
        };
        let about_memory = words[i..words.len().min(i + 4)]
            .iter()
            .any(|x| x.contains("ram") || x.contains("memory"));
        if let (Ok(n), true) = (number.parse::<u64>(), about_memory) {
            return Some(n * GB);
        }
    }
    None
}

/// The verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Enough.
    Ok,
    /// Slow or tight, with the reason.
    Tight(String),
    /// Not enough: do not start silently.
    NotEnough(String),
}

/// Compares needs with the machine.
pub fn estimate(needs: Needs, m: Machine) -> Verdict {
    let gb = |b: u64| format!("{:.1} GB", b as f64 / GB as f64);
    if m.disk_free > 0 && m.disk_free < needs.disk {
        return Verdict::NotEnough(format!(
            "needs ~{} of disk, {} free",
            gb(needs.disk),
            gb(m.disk_free)
        ));
    }
    if m.memory_total > 0 && m.memory_total < needs.memory {
        return Verdict::NotEnough(format!(
            "needs ~{} of memory, the machine has {}",
            gb(needs.memory),
            gb(m.memory_total)
        ));
    }
    if m.memory_available > 0 && m.memory_available < needs.memory {
        return Verdict::Tight(format!(
            "needs ~{} of memory, {} free now — close heavy programs",
            gb(needs.memory),
            gb(m.memory_available)
        ));
    }
    if m.cpus < 2 {
        return Verdict::Tight("one CPU: the build will be slow".into());
    }
    Verdict::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Machine = Machine {
        cpus: 8,
        memory_total: 16 * GB,
        memory_available: 8 * GB,
        disk_free: 100 * GB,
    };

    #[test]
    fn verdicts() {
        let gradle = profile(Ecosystem::Gradle);
        assert_eq!(estimate(gradle, M), Verdict::Ok);
        assert!(
            matches!(estimate(gradle, Machine { disk_free: GB, ..M }), Verdict::NotEnough(r) if r.contains("disk"))
        );
        assert!(matches!(
            estimate(
                gradle,
                Machine {
                    memory_total: 2 * GB,
                    ..M
                }
            ),
            Verdict::NotEnough(_)
        ));
        assert!(matches!(
            estimate(
                gradle,
                Machine {
                    memory_available: GB,
                    ..M
                }
            ),
            Verdict::Tight(_)
        ));
        assert!(matches!(
            estimate(gradle, Machine { cpus: 1, ..M }),
            Verdict::Tight(_)
        ));
    }

    #[test]
    fn hints() {
        assert_eq!(
            agents_memory_hint("The build needs 8 GB RAM."),
            Some(8 * GB)
        );
        assert_eq!(agents_memory_hint("Requires 16GB of memory"), Some(16 * GB));
        assert_eq!(agents_memory_hint("Download is 2 GB"), None);
    }

    #[test]
    fn this_machine_is_measured() {
        let m = Machine::measure(&std::env::temp_dir());
        assert!(m.cpus >= 1 && m.memory_total > 0);
        assert!(m.disk_free > 0, "{m:?}");
    }
}
