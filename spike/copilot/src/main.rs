//! Spike: a user token of the Vibefork.me GitHub App (device flow) drives the Copilot SDK.
//! The token is kept in the OS secret store under a spike-only entry and is never printed.
use std::sync::Arc;
use std::time::Duration;

use github_copilot_sdk::handler::DenyAllHandler;
use github_copilot_sdk::types::{MessageOptions, SessionConfig};
use github_copilot_sdk::{Client, ClientOptions};

const ENTRY: (&str, &str) = ("vibefork/spike-copilot", "github");

fn token() -> String {
    let entry = keyring::Entry::new(ENTRY.0, ENTRY.1).expect("keyring");
    if std::env::args().all(|a| a != "--login") {
        if let Ok(t) = entry.get_password() {
            let t: vf_github::Tokens = serde_json::from_str(&t).expect("stored tokens");
            if !t.expiring(Duration::from_secs(120)) {
                return t.access;
            }
            if let Ok(n) = vf_github::refresh(&t) {
                entry.set_password(&serde_json::to_string(&n).unwrap()).unwrap();
                println!("token refreshed");
                return n.access;
            }
        }
    }
    let code = vf_github::start_device_flow().expect("device flow");
    println!("Open {} and enter {}", code.verification_uri, code.user_code);
    let t = vf_github::poll_token(&code, &vf_protocol::Cancel::default()).expect("sign-in");
    entry.set_password(&serde_json::to_string(&t).unwrap()).unwrap();
    println!("signed in (token prefix {})", &t.access[..4]);
    t.access
}

fn main() {
    let tok = token();
    tokio::runtime::Runtime::new().unwrap().block_on(run(tok));
}

async fn run(tok: String) {
    let opts = ClientOptions::new().with_github_token(tok).with_use_logged_in_user(false);
    let client = match Client::start(opts).await {
        Ok(c) => c,
        Err(e) => return println!("client start failed: {e}"),
    };
    match client.get_auth_status().await {
        Ok(s) => println!(
            "auth: authenticated={} type={:?} login={:?} message={:?}",
            s.is_authenticated, s.auth_type, s.login, s.status_message
        ),
        Err(e) => println!("auth status failed: {e}"),
    }
    match client.list_models().await {
        Ok(models) => {
            println!("models: {}", models.len());
            for m in &models {
                let v = serde_json::to_value(m).unwrap();
                println!("  {} | {} | billing={}", v["id"], v["name"], v["billing"]);
            }
        }
        Err(e) => println!("list models failed: {e}"),
    }
    let model = std::env::args().skip_while(|a| a != "--model").nth(1);
    let mut cfg = SessionConfig::default().with_permission_handler(Arc::new(DenyAllHandler));
    if let Some(m) = &model {
        cfg = cfg.with_model(m.clone());
    }
    let session = match client.create_session(cfg).await {
        Ok(s) => s,
        Err(e) => return println!("create session failed: {e}"),
    };
    let mut events = session.subscribe();
    let log = tokio::spawn(async move {
        while let Ok(ev) = events.recv().await {
            match ev.event_type.as_str() {
                "assistant.usage" | "session.error" | "session.info" | "session.warning"
                | "assistant.message" | "session.model_change" => {
                    println!("event {}: {}", ev.event_type, ev.data)
                }
                t if t.contains("quota") || t.contains("limit") => println!("event {t}: {}", ev.data),
                _ => {}
            }
        }
    });
    let r = session
        .send_and_wait(
            MessageOptions::new("Reply with exactly one word: OK")
                .with_wait_timeout(Duration::from_secs(120)),
        )
        .await;
    println!("send: {}", if r.is_ok() { "ok".into() } else { format!("{:?}", r.err()) });
    tokio::time::sleep(Duration::from_secs(1)).await;
    let _ = session.disconnect().await;
    let _ = client.stop().await;
    log.abort();
}
