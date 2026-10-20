//! Resumable download with a pinned SHA-256: data goes to `<file>.part`, an interrupted download
//! continues with an HTTP `Range` request, the file is renamed into place only after the hash
//! matches.

use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use vf_protocol::Cancel;

/// Downloads `url` to `dest` unless it is already there with the right hash.
pub fn download(url: &str, dest: &Path, sha256: &str, cancel: &Cancel) -> Result<(), String> {
    if dest.exists() && file_hash(dest)? == sha256 {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension("part");
    fetch(url, &part, cancel)?;
    let got = file_hash(&part)?;
    if got != sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("checksum mismatch for {url}: {got}"));
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    std::io::copy(&mut file, &mut h).map_err(|e| e.to_string())?;
    Ok(hex::encode(h.finalize()))
}

/// Appends the rest of `url` to `part` (redirects to the release CDN are followed; the hash is
/// checked afterwards).
fn fetch(url: &str, part: &Path, cancel: &Cancel) -> Result<(), String> {
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(url);
    if have > 0 {
        req = req.header("range", format!("bytes={have}-"));
    }
    let url = url.to_owned();
    let part = part.to_path_buf();
    let cancel = cancel.clone();
    block_on(async move {
        let mut res = req.send().await.map_err(|e| e.to_string())?;
        let resumed = res.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !res.status().is_success() {
            return Err(format!("{url}: HTTP {}", res.status()));
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .map_err(|e| e.to_string())?;
        loop {
            if cancel.is_cancelled() {
                return Err("cancelled".into());
            }
            let chunk = tokio::time::timeout(Duration::from_secs(60), res.chunk())
                .await
                .map_err(|_| "download stalled".to_owned())?
                .map_err(|e| e.to_string())?;
            match chunk {
                Some(bytes) => file.write_all(&bytes).map_err(|e| e.to_string())?,
                None => return Ok(()),
            }
        }
    })?
}

/// Runs a future from a blocking thread (inside the agent's runtime, or a private one).
pub(crate) fn block_on<F: std::future::Future>(f: F) -> Result<F::Output, String> {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => Ok(h.block_on(f)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|rt| rt.block_on(f))
            .map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_file_with_the_right_hash_is_kept() {
        let dir = std::env::temp_dir().join(format!("vf-dl-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("x.bin");
        assert!(std::fs::write(&f, b"abc").is_ok());
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(
            download("http://127.0.0.1:9/never", &f, abc, &Cancel::new()).is_ok(),
            "no network needed"
        );
        assert!(
            download(
                "http://127.0.0.1:9/never",
                &f,
                &"0".repeat(64),
                &Cancel::new()
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
