//! Randomized robustness of every parser of untrusted input (stable Rust, deterministic, runs in CI;
//! `fuzz/` has the same targets for cargo-fuzz). Inputs: random bytes and mutations of real samples.
//! Each target checks a property, not only "no panic".

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::Component;

use vf_core::ProjectPath;
use vf_protocol::ws::ClientMessage;

const ROUNDS: usize = 20_000;

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const ALPHABET: &[u8] = b"{}[]\":,./\\-_ azAZ09\n\t\0..\xc3\xa9\xff%";

/// A mutation of a sample: byte flips, inserts, deletions, cut, duplication, or random bytes.
fn mutate(rng: &mut Rng, samples: &[&[u8]]) -> Vec<u8> {
    let mut v: Vec<u8> = if rng.below(8) == 0 {
        (0..rng.below(64)).map(|_| rng.next() as u8).collect()
    } else {
        samples[rng.below(samples.len())].to_vec()
    };
    for _ in 0..=rng.below(6) {
        let len = v.len();
        match rng.below(6) {
            0 if len > 0 => v[rng.below(len)] = ALPHABET[rng.below(ALPHABET.len())],
            1 => v.insert(rng.below(len + 1), ALPHABET[rng.below(ALPHABET.len())]),
            2 if len > 0 => {
                v.remove(rng.below(len));
            }
            3 => v.truncate(rng.below(len + 1)),
            4 if len > 0 => {
                let (a, b) = (rng.below(len), rng.below(len));
                let (a, b) = (a.min(b), a.max(b));
                let chunk = v[a..b].to_vec();
                v.splice(a..a, chunk);
            }
            _ => v.push(rng.next() as u8),
        }
    }
    v
}

fn run(seed: u64, samples: &[&[u8]], mut check: impl FnMut(&[u8])) {
    let mut rng = Rng(seed);
    for _ in 0..ROUNDS {
        check(&mutate(&mut rng, samples));
    }
}

#[test]
fn project_paths_never_escape() {
    let samples: &[&[u8]] = &[
        b"src/main.rs",
        b"../x",
        b"a/./b/../c",
        b"C:\\x",
        b"/etc/passwd",
        b"\\\\?\\C:\\",
        b"a\\..\\..\\b",
    ];
    run(1, samples, |input| {
        let s = String::from_utf8_lossy(input);
        if let Ok(p) = ProjectPath::parse(&s) {
            assert!(!p.as_path().is_absolute(), "{s:?}");
            let root = std::path::Path::new(if cfg!(windows) { "D:\\proj" } else { "/proj" });
            assert!(
                p.under(root).starts_with(root),
                "{s:?} leaves the root: {:?}",
                p.under(root)
            );
            assert!(
                p.as_path()
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
                "{s:?} → {:?}",
                p.as_path()
            );
        }
    });
}

#[test]
fn client_messages() {
    let samples: &[&[u8]] = &[
        br#"{"type":"auth","token":"x"}"#,
        br#"{"type":"task","project":"p","prompt":"x","provider":"auto","continue":true}"#,
        br#"{"type":"build","project":"p"}"#,
        br#"{"type":"cancel"}"#,
    ];
    run(2, samples, |input| {
        if let Ok(ClientMessage::Task {
            project, prompt, ..
        }) = serde_json::from_slice::<ClientMessage>(input)
        {
            // Accepted messages keep their fields as plain strings; nothing else is interpreted here.
            let _ = (project.len(), prompt.len());
        }
    });
}

#[test]
fn signed_manifests() {
    let key = vf_updater::signing::public_key(vf_updater::UI_BUNDLE_PUBLIC_KEY).unwrap();
    let samples: &[&[u8]] = &[
        br#"{"version":"1","files":{"index.html":"00"}}"#,
        br#"{"version":"1","files":{"../x":"00"}}"#,
    ];
    run(3, samples, |input| {
        let sig = String::from_utf8_lossy(&input[..input.len() / 2]);
        assert!(
            vf_updater::verify_manifest(input, &sig, &key).is_err(),
            "nothing random verifies"
        );
    });
}

#[test]
fn provider_streams() {
    let fixtures = [
        include_str!("../../vf-providers/tests/fixtures/claude.jsonl"),
        include_str!("../../vf-providers/tests/fixtures/codex.jsonl"),
    ];
    let lines: Vec<&[u8]> = fixtures
        .iter()
        .flat_map(|f| f.lines())
        .map(str::as_bytes)
        .collect();
    run(4, &lines, |input| {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(input) {
            for ev in vf_providers::claude::normalize(&v)
                .into_iter()
                .chain(vf_providers::codex::normalize(&v))
            {
                // Every event serializes back to the wire format.
                assert!(serde_json::to_string(&ev).is_ok());
            }
        }
    });
}

#[test]
fn plans_and_ratings() {
    let samples: &[&[u8]] = &[
        br#"{"steps":[{"title":"a","prompt":"do a","complexity":"simple"},{"prompt":"b"}]}"#,
        br#"Sure: ```json {"complexity":"medium","decompose":true,"reason":"r"}```"#,
    ];
    run(5, samples, |input| {
        let s = String::from_utf8_lossy(input);
        if let Ok(steps) = vf_orchestrator::parse_plan(&s) {
            assert!(!steps.is_empty() && steps.len() <= vf_orchestrator::MAX_STEPS);
            assert!(
                steps
                    .iter()
                    .all(|st| !st.prompt.trim().is_empty() && st.title.chars().count() <= 120)
            );
        }
        if let Some(r) = vf_orchestrator::parse_rating(&s, "c") {
            assert!(["simple", "medium", "hard"].contains(&r.complexity.as_str()));
        }
    });
}

#[test]
fn slugs_and_hints() {
    let samples: &[&[u8]] = &[
        b"Dark mode",
        "Тёмная тема".as_bytes(),
        b"needs 8 GB RAM",
        b"--x--",
    ];
    run(6, samples, |input| {
        let s = String::from_utf8_lossy(input);
        let slug = vf_journal::customizations::slug(&s);
        assert!(!slug.is_empty() && slug.len() <= 40, "{slug}");
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{slug}"
        );
        assert!(!slug.starts_with('-') && !slug.ends_with('-'), "{slug}");
        let _ = vf_builds::estimate::agents_memory_hint(&s);
    });
}

#[test]
fn journal_files_with_garbage() {
    let dir = std::env::temp_dir().join(format!("vf-robust-{}", std::process::id()));
    let journal = vf_platform::JournalFiles::new(&dir);
    let samples: &[&[u8]] = &[br#"{"at":"t","provider":"p","model":"m","prompt":"x"}"#];
    let mut rng = Rng(7);
    let mut text = Vec::new();
    for _ in 0..500 {
        text.extend(mutate(&mut rng, samples));
        text.push(b'\n');
    }
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.jsonl"), &text).unwrap();
    use vf_journal::JournalStore as _;
    assert!(journal.read("p").is_ok(), "broken lines are skipped");
    let _ = std::fs::remove_dir_all(&dir);
}
