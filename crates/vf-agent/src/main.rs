//! The `vibefork` binary: **wiring only** — reads settings, builds the parts and hands control to
//! `vf-cli`. Logic belongs in other crates.

use std::process::ExitCode;
use std::sync::{Arc, RwLock};

use vf_auth::Auth;
use vf_core::ConfirmDialog;
use vf_engine::Engine;
use vf_platform::Settings;
use vf_platform::settings::HOSTED_UI_ORIGIN;
use vf_providers::Provider;
use vf_providers::claude::Claude;
use vf_providers::codex::Codex;
use vf_providers::openai::OpenAiCompatible;
use vf_server::{Config, UiSlot};
use vf_updater::UiBundle;

struct Agent {
    settings: Settings,
}

impl vf_cli::App for Agent {
    fn serve(&self) -> ExitCode {
        let s = &self.settings;
        if let Err(e) = s.secure_state_dir() {
            tracing::error!(error = %e, dir = %s.state_dir.display(), "cannot create the state folder");
            return ExitCode::FAILURE;
        }
        let auth = match Auth::load(s.state_dir.clone(), dialog()) {
            Ok(auth) => auth,
            Err(e) => {
                tracing::error!(error = %e, dir = %s.state_dir.display(), "cannot load pairings");
                return ExitCode::FAILURE;
            }
        };
        let Ok(ui_key) = vf_updater::signing::public_key(vf_updater::UI_BUNDLE_PUBLIC_KEY) else {
            tracing::error!("built-in UI bundle key is malformed");
            return ExitCode::FAILURE;
        };
        let ui: UiSlot = Arc::new(RwLock::new(UiBundle::load_cached(
            &s.ui_cache_dir(),
            &ui_key,
        )));
        let mut origins = vec![HOSTED_UI_ORIGIN.to_owned()];
        origins.extend(s.extra_origins.iter().cloned());

        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!(error = %e, "cannot start the async runtime");
                return ExitCode::FAILURE;
            }
        };
        let result = runtime.block_on(async {
            let (listener, port) = vf_server::bind(s.port).await?;
            if let Err(e) = s.save_port(port) {
                tracing::warn!(error = %e, "cannot remember the port");
            }
            if let Err(e) = vf_platform::agent_process::write_pid(&s.state_dir) {
                tracing::warn!(error = %e, "cannot write the pid file");
            }
            if s.auto_update {
                let settings = s.clone();
                std::thread::spawn(move || auto_update(&settings));
            }
            let (url, cache, slot) = (s.ui_bundle_url.clone(), s.ui_cache_dir(), ui.clone());
            tokio::spawn(async move {
                match UiBundle::download(&url, &ui_key, &cache).await {
                    Ok(bundle) => {
                        tracing::info!(version = %bundle.version, "UI bundle verified");
                        *slot.write().unwrap_or_else(|p| p.into_inner()) = Some(bundle);
                    }
                    Err(e) => tracing::warn!(error = %e, %url, "UI bundle not updated"),
                }
            });
            vf_server::serve(
                listener,
                Config {
                    port,
                    allowed_origins: origins,
                },
                auth,
                ui,
                Arc::new(Engine::new(s, providers())),
            )
            .await
        });
        vf_platform::agent_process::clear_pid(&s.state_dir);
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!(error = %e, "server stopped");
                ExitCode::FAILURE
            }
        }
    }
}

/// Checks the signed release channel at start and every 6 hours; a newer version replaces the
/// binaries and takes effect on the next start.
fn auto_update(settings: &Settings) {
    loop {
        match vf_cli::update::update(settings, false) {
            Ok(vf_cli::update::Outcome::Updated(v)) => {
                tracing::info!(version = %v, "update installed; it takes effect on the next start");
                return;
            }
            Ok(_) => {}
            Err(e) => tracing::info!(error = %e, "update check failed"),
        }
        std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
    }
}

/// Providers found on this machine: CLIs on PATH and API keys in the secret store.
fn providers() -> Vec<Arc<dyn Provider>> {
    vec![
        Arc::new(Claude::detect()),
        Arc::new(Codex::detect()),
        Arc::new(OpenAiCompatible::openrouter(
            vf_platform::secrets::openrouter_key(),
        )),
    ]
}

#[cfg(not(feature = "insecure-auto-approve"))]
fn dialog() -> Arc<dyn ConfirmDialog> {
    Arc::new(vf_platform::NativeConfirmDialog)
}

/// Test builds: `VF_AUTO_APPROVE=1` approves every dialog without showing it.
#[cfg(feature = "insecure-auto-approve")]
fn dialog() -> Arc<dyn ConfirmDialog> {
    if std::env::var("VF_AUTO_APPROVE").is_ok_and(|v| v == "1") {
        Arc::new(vf_platform::dialog::AutoApproveDialog)
    } else {
        Arc::new(vf_platform::NativeConfirmDialog)
    }
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("VF_LOG").unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    vf_cli::run(
        &args,
        &Agent {
            settings: Settings::from_env(),
        },
    )
}
