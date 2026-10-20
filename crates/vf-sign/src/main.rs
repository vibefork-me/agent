//! `vibefork-sign`: offline signing tool for UI bundles and agent releases. Not shipped to users.
//!
//! ```text
//! vibefork-sign keygen <secret-key-file> <public-key-file>
//! vibefork-sign bundle <dir> <secret-key-file> <version>    # UI bundle: writes <dir>/manifest.json and manifest.sig
//! vibefork-sign release <dir> <secret-key-file> <version>   # release channel: the same for the binaries in <dir>
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use vf_updater::Manifest;
use vf_updater::signing::{sha256_hex, sign};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", secret, public] => keygen(Path::new(secret), Path::new(public)),
        ["bundle" | "release", dir, secret, version] => {
            bundle(Path::new(dir), Path::new(secret), version)
        }
        _ => Err(
            "usage: vibefork-sign keygen <secret> <public> | bundle|release <dir> <secret> <version>"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vibefork-sign: {e}");
            ExitCode::FAILURE
        }
    }
}

fn keygen(secret: &Path, public: &Path) -> Result<(), String> {
    if secret.exists() {
        return Err(format!("{} already exists", secret.display()));
    }
    let key = SigningKey::generate(&mut OsRng);
    std::fs::write(secret, STANDARD.encode(key.to_bytes())).map_err(|e| e.to_string())?;
    std::fs::write(
        public,
        format!("{}\n", STANDARD.encode(key.verifying_key().to_bytes())),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "secret key: {} (keep offline)\npublic key: {}",
        secret.display(),
        public.display()
    );
    Ok(())
}

fn bundle(dir: &Path, secret: &Path, version: &str) -> Result<(), String> {
    let raw = std::fs::read_to_string(secret).map_err(|e| e.to_string())?;
    let bytes: [u8; 32] = STANDARD
        .decode(raw.trim())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "bad secret key")?;
    let key = SigningKey::from_bytes(&bytes);
    let mut files = BTreeMap::new();
    for path in walk(dir)? {
        let rel = path
            .strip_prefix(dir)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "manifest.json" || rel == "manifest.sig" {
            continue;
        }
        files.insert(
            rel,
            sha256_hex(&std::fs::read(&path).map_err(|e| e.to_string())?),
        );
    }
    let manifest = Manifest {
        version: version.to_owned(),
        files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), &manifest_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.sig"), sign(&manifest_bytes, &key))
        .map_err(|e| e.to_string())?;
    println!("signed {} files, version {version}", manifest.files.len());
    Ok(())
}

fn walk(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}
