//! `vibefork-sign`: offline signing tool for UI bundles and agent releases. Not shipped to users.
//!
//! ```text
//! vibefork-sign keygen <secret-key-file> <public-key-file>
//! vibefork-sign bundle <dir> <secret-key-file> <version> [--days <N>]
//! vibefork-sign release <dir> <secret-key-file> <version> [--days <N>]
//! vibefork-sign certify-subkey <root-secret> <subkey-public> <purpose> <days>
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use vf_updater::signing::{sha256_hex, sign, sign_subkey_cert};
use vf_updater::{Manifest, SubkeyCert};

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
            bundle(Path::new(dir), Path::new(secret), version, 90)
        }
        ["bundle" | "release", dir, secret, version, "--days", days] => days
            .parse::<i64>()
            .map_err(|_| "--days must be a positive integer".to_owned())
            .and_then(|days| bundle(Path::new(dir), Path::new(secret), version, days)),
        ["certify-subkey", root, public, purpose, days] => days
            .parse::<i64>()
            .map_err(|_| "days must be a positive integer".to_owned())
            .and_then(|days| certify_subkey(Path::new(root), Path::new(public), purpose, days)),
        _ => Err("usage: vibefork-sign keygen <secret> <public> | bundle|release <dir> <secret> <version> [--days <N>] | certify-subkey <root-secret> <subkey-public> <purpose> <days>".into()),
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

fn bundle(dir: &Path, secret: &Path, version: &str, days: i64) -> Result<(), String> {
    if days <= 0 {
        return Err("--days must be a positive integer".into());
    }
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
        expires_at: Some(expiry(days)?),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.json"), &manifest_bytes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("manifest.sig"), sign(&manifest_bytes, &key))
        .map_err(|e| e.to_string())?;
    println!("signed {} files, version {version}", manifest.files.len());
    Ok(())
}

fn certify_subkey(
    root_secret: &Path,
    subkey_public: &Path,
    purpose: &str,
    days: i64,
) -> Result<(), String> {
    if purpose != "bundle" && purpose != "release" {
        return Err("purpose must be bundle or release".into());
    }
    if days <= 0 {
        return Err("days must be a positive integer".into());
    }
    let root = read_signing_key(root_secret)?;
    let public_key = std::fs::read_to_string(subkey_public).map_err(|e| e.to_string())?;
    // Validate the supplied public key before certifying it.
    vf_updater::signing::public_key(public_key.trim()).map_err(|e| e.to_string())?;
    let cert = SubkeyCert {
        public_key: public_key.trim().to_owned(),
        expires_at: expiry(days)?,
        purpose: purpose.to_owned(),
    };
    let cert_json = serde_json::to_vec_pretty(&cert).map_err(|e| e.to_string())?;
    let signature = sign_subkey_cert(&cert_json, &root);
    let base = subkey_public.as_os_str().to_string_lossy();
    std::fs::write(format!("{base}.cert.json"), &cert_json).map_err(|e| e.to_string())?;
    std::fs::write(format!("{base}.cert.sig"), signature).map_err(|e| e.to_string())?;
    println!("certified {purpose} subkey until {}", cert.expires_at);
    Ok(())
}

fn read_signing_key(path: &Path) -> Result<SigningKey, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let bytes: [u8; 32] = STANDARD
        .decode(raw.trim())
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "bad secret key")?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn expiry(days: i64) -> Result<i64, String> {
    if days <= 0 {
        return Err("days must be a positive integer".into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let now = i64::try_from(now).map_err(|_| "system clock is out of range")?;
    now.checked_add(days.checked_mul(86_400).ok_or("days is too large")?)
        .ok_or_else(|| "expiry is out of range".into())
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
