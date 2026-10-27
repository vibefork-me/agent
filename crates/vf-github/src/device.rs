//! Device flow: the agent asks GitHub for a code, the user enters it on github.com, the agent
//! receives the token. Tokens refresh without the app's secret (device-flow tokens).

use std::time::Duration;

use serde_json::Value;
use vf_core::Secret;
use vf_protocol::Cancel;

use crate::{CLIENT_ID, GithubError, Tokens, block_on, http, now, read_json};

/// A started sign-in.
#[derive(Debug)]
pub struct DeviceCode {
    /// Code the user enters on GitHub.
    pub user_code: String,
    /// Where to enter it.
    pub verification_uri: String,
    /// Seconds the code is valid.
    pub expires_in: u64,
    /// Seconds between polls.
    pub interval: u64,
    device_code: Secret<String>,
}

fn form(url: &str, params: &[(&str, &str)]) -> Result<Value, GithubError> {
    let req = http()?
        .post(url)
        .header("accept", "application/json")
        .form(params);
    block_on(async {
        let res = req
            .send()
            .await
            .map_err(|e| GithubError::Http(e.to_string()))?;
        read_json(res).await
    })?
}

fn tokens(v: &Value) -> Option<Tokens> {
    Some(Tokens {
        access: v["access_token"].as_str()?.to_owned(),
        refresh: v["refresh_token"].as_str().map(str::to_owned),
        expires_at: v["expires_in"].as_u64().map(|s| now() + s),
    })
}

/// Asks GitHub for a sign-in code.
pub fn start_device_flow() -> Result<DeviceCode, GithubError> {
    let v = form(
        "https://github.com/login/device/code",
        &[("client_id", CLIENT_ID)],
    )?;
    let field = |k: &str| {
        v[k].as_str().map(str::to_owned).ok_or_else(|| {
            GithubError::Auth(
                v["error_description"]
                    .as_str()
                    .unwrap_or("GitHub refused the sign-in")
                    .to_owned(),
            )
        })
    };
    Ok(DeviceCode {
        user_code: field("user_code")?,
        verification_uri: field("verification_uri")?,
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
        interval: v["interval"].as_u64().unwrap_or(5),
        device_code: Secret::new(field("device_code")?),
    })
}

/// Waits until the user approves the code (or it expires, or `cancel`).
pub fn poll_token(code: &DeviceCode, cancel: &Cancel) -> Result<Tokens, GithubError> {
    let deadline = now() + code.expires_in;
    let mut interval = code.interval.max(1);
    loop {
        if cancel.wait(Duration::from_secs(interval)) {
            return Err(GithubError::Auth("cancelled".into()));
        }
        if now() > deadline {
            return Err(GithubError::Auth("the code expired".into()));
        }
        let v = form(
            "https://github.com/login/oauth/access_token",
            &[
                ("client_id", CLIENT_ID),
                ("device_code", code.device_code.expose()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        if let Some(t) = tokens(&v) {
            return Ok(t);
        }
        match v["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some(e) => return Err(GithubError::Auth(e.replace('_', " "))),
            None => return Err(GithubError::Auth("no answer from GitHub".into())),
        }
    }
}

/// A new access token from the refresh token.
pub fn refresh(old: &Tokens) -> Result<Tokens, GithubError> {
    let Some(r) = &old.refresh else {
        return Err(GithubError::Auth("GitHub sign-in expired".into()));
    };
    let v = form(
        "https://github.com/login/oauth/access_token",
        &[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", r),
        ],
    )?;
    tokens(&v).ok_or_else(|| GithubError::Auth("GitHub sign-in expired".into()))
}
