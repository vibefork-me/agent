//! The agent's own coding loop over an OpenAI-compatible chat API (OpenRouter): the model gets file
//! and shell tools limited to the project folder. Works with nothing installed but an API key.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vf_core::{LimitedBody, Secret, limits};
use vf_protocol::{Cancel, CostClass, Event, ModelInfo, Usage};

use crate::tools::{self, Outcome};
use crate::{Answer, AskRequest, Output, Provider, RunRequest, RunResult, Tracker, clip};

/// OpenRouter API.
pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
/// Google AI Studio (Gemini) OpenAI-compatible API.
pub const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
/// Most model turns of one run.
pub const MAX_STEPS: usize = 30;
/// How long the model list is cached.
const MODELS_TTL: Duration = Duration::from_secs(600);
/// Timeout of one model request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// An OpenAI-compatible API with a key.
pub struct OpenAiCompatible {
    id: &'static str,
    base_url: Option<String>,
    base_url_source: Option<Box<dyn Fn() -> Option<String> + Send + Sync>>,
    model_source: Option<Box<dyn Fn() -> Option<String> + Send + Sync>>,
    key_source: Box<dyn Fn() -> Option<Secret<String>> + Send + Sync>,
    cache: Mutex<Option<(Instant, Vec<ModelInfo>)>>,
    /// Whether the API accepted the current key (`GET /key`): when checked, a fingerprint of the
    /// key (not the key), the answer.
    accepted: Mutex<Option<(Instant, u64, bool)>>,
}

impl OpenAiCompatible {
    /// OpenRouter with a key (none = no models).
    pub fn openrouter(key: Option<Secret<String>>) -> Self {
        Self::new("openrouter", OPENROUTER_URL, key)
    }

    /// OpenRouter with dynamic key loading.
    pub fn openrouter_dynamic(
        key_source: impl Fn() -> Option<Secret<String>> + Send + Sync + 'static,
    ) -> Self {
        Self::dynamic("openrouter", OPENROUTER_URL, key_source)
    }

    /// Google AI Studio (Gemini) with a key (none = no models).
    pub fn gemini(key: Option<Secret<String>>) -> Self {
        Self::new("gemini", GEMINI_URL, key)
    }

    /// Google AI Studio (Gemini) with dynamic key loading.
    pub fn gemini_dynamic(
        key_source: impl Fn() -> Option<Secret<String>> + Send + Sync + 'static,
    ) -> Self {
        Self::dynamic("gemini", GEMINI_URL, key_source)
    }

    /// Custom OpenAI-compatible provider with dynamic configuration.
    /// `config_source` returns `Option<(endpoint_url, optional_model)>`.
    pub fn custom_dynamic(
        config_source: impl Fn() -> Option<(String, Option<String>)> + Send + Sync + 'static,
        key_source: impl Fn() -> Option<Secret<String>> + Send + Sync + 'static,
    ) -> Self {
        let config_source = std::sync::Arc::new(config_source);
        let cs1 = config_source.clone();
        let cs2 = config_source;
        Self {
            id: "custom",
            base_url: None,
            base_url_source: Some(Box::new(move || cs1().map(|c| c.0))),
            model_source: Some(Box::new(move || cs2().and_then(|c| c.1))),
            key_source: Box::new(key_source),
            cache: Mutex::new(None),
            accepted: Mutex::new(None),
        }
    }

    /// Any compatible API.
    pub fn new(id: &'static str, base_url: &str, key: Option<Secret<String>>) -> Self {
        Self::dynamic(id, base_url, move || key.clone())
    }

    /// Any compatible API with dynamic key loading.
    pub fn dynamic(
        id: &'static str,
        base_url: &str,
        key_source: impl Fn() -> Option<Secret<String>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            id,
            base_url: Some(base_url.trim_end_matches('/').to_owned()),
            base_url_source: None,
            model_source: None,
            key_source: Box::new(key_source),
            cache: Mutex::new(None),
            accepted: Mutex::new(None),
        }
    }

    /// Returns the effective base URL, if configured.
    pub fn base_url(&self) -> Option<String> {
        if let Some(src) = &self.base_url_source {
            src().map(|u| u.trim_end_matches('/').to_owned())
        } else {
            self.base_url.clone()
        }
    }

    /// Configured default model identifier, if any.
    pub fn configured_model(&self) -> Option<String> {
        self.model_source.as_ref().and_then(|s| s())
    }

    /// The API key, if present.
    pub fn key(&self) -> Option<Secret<String>> {
        // A stored key may carry a byte order mark or spaces from the dialog that read it; an empty
        // key is no key (the API would answer "Missing Authentication header").
        (self.key_source)()
            .map(|k| Secret::new(vf_core::clean_secret(k.expose())))
            .filter(|k| !k.expose().is_empty())
    }

    /// Clears any cached model list and key check.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        *cache = None;
        *self.accepted.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// Whether the API accepts this key, checked with `GET /key` at most every [`MODELS_TTL`].
    /// A network failure is not a verdict: the key counts as accepted until the API says no.
    fn accepted(&self, key: &Secret<String>) -> bool {
        use std::hash::{Hash as _, Hasher as _};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.expose().hash(&mut h);
        let print = h.finish();
        let mut cache = self.accepted.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, p, ok)) = *cache
            && p == print
            && at.elapsed() < MODELS_TTL
        {
            return ok;
        }
        let base_url = match self.base_url() {
            Some(u) => u,
            None => return false,
        };
        let ok = match key_accepted(self.id, &base_url, key) {
            Ok(ok) => ok,
            Err(e) => {
                tracing::warn!(error = %e, provider = self.id, "key not checked");
                return true;
            }
        };
        if !ok {
            tracing::warn!(provider = self.id, shape = %key_shape(key.expose()), "the API does not accept the key");
        }
        *cache = Some((Instant::now(), print, ok));
        ok
    }

    fn client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| e.to_string())
    }

    /// Validates an API key: `GET /key` answers only to a real key (`/models` is public and accepts
    /// anything), then fetches the free models.
    pub fn verify_key(base_url: &str, key: &Secret<String>) -> Result<Vec<ModelInfo>, String> {
        Self::verify_key_for("openrouter", base_url, key)
    }

    /// Validates an API key for a specified provider (`openrouter`, `gemini`), then fetches models.
    pub fn verify_key_for(
        id: &str,
        base_url: &str,
        key: &Secret<String>,
    ) -> Result<Vec<ModelInfo>, String> {
        if key.expose().trim().is_empty() {
            return Err("the key is empty".into());
        }
        let base = base_url.trim_end_matches('/');
        if !key_accepted(id, base, key)? {
            let name = if id == "gemini" {
                "Google AI Studio"
            } else {
                "OpenRouter"
            };
            return Err(format!("{name} did not accept the key"));
        }
        let url = format!("{base}/models");
        let body: Value = block_on(async {
            let res = Self::client()?
                .get(&url)
                .bearer_auth(key.expose())
                .header("x-title", "vibefork")
                .send()
                .await
                .map_err(|_| format!("network error contacting {id}"))?;
            let status = res.status();
            if !status.is_success() {
                return Err(format!("{id} returned HTTP {status}"));
            }
            read_json(res)
                .await
                .map_err(|_| format!("invalid JSON response from {id}"))
        })??;
        if id == "gemini" {
            Ok(gemini_models(id, &body))
        } else {
            Ok(free_tool_models(id, &body))
        }
    }

    /// Verifies a custom OpenAI-compatible endpoint.
    /// Tries `GET {endpoint}/models`. If that yields models, returns them.
    /// If `/models` fails or yields no models, and a `model` identifier is given,
    /// verifies with a minimal chat completion (`POST {endpoint}/chat/completions`).
    pub fn verify_custom_endpoint(
        endpoint: &str,
        key: Option<&str>,
        model: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let base = endpoint.trim_end_matches('/');
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        let models_url = format!("{base}/models");
        let models_res: Result<Value, String> = block_on(async {
            let mut req = client.get(&models_url).header("x-title", "vibefork");
            if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
                req = req.bearer_auth(k);
            }
            let res = req.send().await.map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                return Err(format!("HTTP {}", res.status()));
            }
            read_json(res).await
        })?;

        let mut models = Vec::new();
        if let Ok(v) = models_res {
            let list = v["data"].as_array().or_else(|| v["models"].as_array());
            if let Some(arr) = list {
                for item in arr {
                    let id = item["id"]
                        .as_str()
                        .or_else(|| item["name"].as_str())
                        .or_else(|| item["model"].as_str());
                    if let Some(id) = id {
                        models.push(id.to_owned());
                    }
                }
            }
        }

        if !models.is_empty() {
            return Ok(models);
        }

        // If no models from /models, we require a model identifier and test a chat completion
        let Some(m) = model.filter(|m| !m.trim().is_empty()) else {
            return Err(
                "Endpoint does not return any models from /models; a model identifier is required."
                    .into(),
            );
        };

        let chat_url = format!("{base}/chat/completions");
        let test_body = json!({
            "model": m,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1
        });

        block_on(async {
            let mut req = client
                .post(&chat_url)
                .header("x-title", "vibefork")
                .json(&test_body);
            if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
                req = req.bearer_auth(k);
            }
            let res = req
                .send()
                .await
                .map_err(|e| format!("Failed to reach endpoint: {e}"))?;
            let status = res.status();
            if !status.is_success() {
                let err_text = read_error_text(res).await;
                return Err(format!("Endpoint returned HTTP {status}: {err_text}"));
            }
            Ok(vec![m.to_owned()])
        })?
    }

    fn fetch_models_with_key(&self, key: &Secret<String>) -> Result<Vec<ModelInfo>, String> {
        let base_url = self.base_url().ok_or("no base URL configured")?;
        Self::verify_key_for(self.id, &base_url, key)
    }

    fn fetch_custom_models(&self, base_url: &str) -> Vec<ModelInfo> {
        let key = self.key();
        let configured_model = self.configured_model();
        let url = format!("{base_url}/models");
        let body: Result<Value, String> = block_on(async {
            let mut req = Self::client()?.get(&url).header("x-title", "vibefork");
            if let Some(k) = key.as_ref().filter(|k| !k.expose().trim().is_empty()) {
                req = req.bearer_auth(k.expose());
            }
            let res = req.send().await.map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                return Err(format!("HTTP {}", res.status()));
            }
            read_json(res).await
        })
        .unwrap_or_else(Err);

        let mut models = Vec::new();
        if let Ok(v) = body {
            let list = v["data"].as_array().or_else(|| v["models"].as_array());
            if let Some(arr) = list {
                for item in arr {
                    let id = item["id"]
                        .as_str()
                        .or_else(|| item["name"].as_str())
                        .or_else(|| item["model"].as_str());
                    if let Some(id) = id {
                        models.push(ModelInfo {
                            provider: self.id.into(),
                            model: id.into(),
                            tier: 2,
                            cost: CostClass::Paid,
                            label: format!("Custom {id}"),
                        });
                    }
                }
            }
        }

        if let Some(cfg) = configured_model
            && !models.iter().any(|m| m.model == cfg)
        {
            models.insert(
                0,
                ModelInfo {
                    provider: self.id.into(),
                    model: cfg.clone(),
                    tier: 2,
                    cost: CostClass::Paid,
                    label: format!("Custom {cfg}"),
                },
            );
        }
        models
    }
}

/// Free models that support tools, from a `/models` response.
pub fn free_tool_models(provider: &str, body: &Value) -> Vec<ModelInfo> {
    body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            let id = m["id"].as_str().unwrap_or_default();
            let zero = |k: &str| m["pricing"][k].as_str() == Some("0");
            let tools = m["supported_parameters"]
                .as_array()
                .is_some_and(|p| p.iter().any(|x| x == "tools"));
            (id.ends_with(":free") || (zero("prompt") && zero("completion"))) && tools
        })
        .filter_map(|m| {
            let id = m["id"].as_str()?;
            Some(ModelInfo {
                provider: provider.into(),
                model: id.into(),
                tier: 1,
                cost: CostClass::Free,
                label: format!("OpenRouter {id}"),
            })
        })
        .collect()
}

/// Free Gemini models from a Google AI Studio `/models` response.
pub fn gemini_models(provider: &str, body: &Value) -> Vec<ModelInfo> {
    let mut out = Vec::new();
    if let Some(arr) = body["data"].as_array() {
        for m in arr {
            let Some(raw_id) = m["id"].as_str() else {
                continue;
            };
            let id = raw_id.strip_prefix("models/").unwrap_or(raw_id);
            if id.starts_with("gemini-2.0-flash")
                || id.starts_with("gemini-1.5-flash")
                || id.starts_with("gemini-2.5-flash")
            {
                out.push(ModelInfo {
                    provider: provider.into(),
                    model: id.into(),
                    tier: 1,
                    cost: CostClass::Free,
                    label: format!("Google {id}"),
                });
            }
        }
    }
    if out.is_empty() {
        out.push(ModelInfo {
            provider: provider.into(),
            model: "gemini-2.0-flash".into(),
            tier: 1,
            cost: CostClass::Free,
            label: "Google Gemini 2.0 Flash".into(),
        });
    }
    out.sort_by_key(|m| if m.model.contains("2.0") { 0 } else { 1 });
    out
}

/// Runs a future from a blocking thread (inside the agent's runtime, or a private one in tests).
fn block_on<F: Future>(f: F) -> Result<F::Output, String> {
    match tokio::runtime::Handle::try_current() {
        Ok(h) => Ok(h.block_on(f)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|rt| rt.block_on(f))
            .map_err(|e| e.to_string()),
    }
}

/// Reads a response body of at most `limit` bytes ([`vf_core::limits`]).
async fn read_body(mut res: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    let mut body = LimitedBody::new(limit, res.content_length()).map_err(|e| e.to_string())?;
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        body.push(&chunk).map_err(|e| e.to_string())?;
    }
    Ok(body.into_vec())
}

/// Reads a JSON body of at most [`limits::API_RESPONSE`] bytes.
async fn read_json(res: reqwest::Response) -> Result<Value, String> {
    let bytes = read_body(res, limits::API_RESPONSE).await?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// Error text of a failed response, at most 64 KiB read and 300 characters kept.
async fn read_error_text(res: reqwest::Response) -> String {
    match read_body(res, 64 * 1024).await {
        Ok(bytes) => clip(String::from_utf8_lossy(&bytes).trim(), 300),
        Err(e) => e,
    }
}

/// One chat request; temporary failures (rate limit, upstream errors) are retried twice.
async fn chat(
    client: &reqwest::Client,
    url: &str,
    key: Option<&Secret<String>>,
    body: &Value,
) -> Result<Value, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = async {
            let mut req = client.post(url).header("x-title", "vibefork").json(body);
            if let Some(k) = key.filter(|k| !k.expose().trim().is_empty()) {
                req = req.bearer_auth(k.expose());
            }
            let res = req.send().await.map_err(|e| (true, e.to_string()))?;
            let status = res.status();
            let v = read_json(res).await.map_err(|e| (true, e))?;
            if !status.is_success() || !v["error"].is_null() {
                let mut message = v["error"]["message"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("HTTP {status}"));
                // OpenRouter puts the upstream provider's own words here ("rate-limited", …).
                if let Some(raw) = v["error"]["metadata"]["raw"].as_str() {
                    message = format!("{message}: {}", clip(raw.trim(), 200));
                }
                let temporary = status.as_u16() == 429
                    || status.is_server_error()
                    || message.contains("Provider returned error");
                return Err((temporary, message));
            }
            Ok(v)
        }
        .await;
        match result {
            Ok(v) => return Ok(v),
            Err((true, e)) if attempt < 3 => {
                tracing::info!(attempt, error = %e, "model request failed, retrying");
                tokio::time::sleep(Duration::from_secs(2 * attempt)).await;
            }
            Err((_, e)) => return Err(e),
        }
    }
}

/// Resolves when `cancel` is set.
async fn cancelled(cancel: &Cancel) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Renders the system prompt for the OpenAI-compatible loop with optional extra instructions.
pub fn system_prompt(instruction: Option<&str>) -> String {
    vf_core::prompts::get(vf_core::prompts::Stage::Loop).render_loop(instruction)
}

impl Provider for OpenAiCompatible {
    fn id(&self) -> &'static str {
        self.id
    }

    fn status(&self) -> crate::Status {
        if self.id == "custom" {
            let connected = self.base_url().is_some();
            return crate::Status {
                installed: true,
                signed_in: Some(connected),
                user_signed_in: None,
                cli_home_mode: None,
            };
        }
        // Signed in = a key the API accepts, not just any stored value.
        let accepted = self.key().is_some_and(|k| self.accepted(&k));
        crate::Status {
            installed: true,
            signed_in: Some(accepted),
            user_signed_in: None,
            cli_home_mode: None,
        }
    }

    fn reload(&self) {
        self.clear_cache();
    }

    fn connect_key(&self, key: &Secret<String>) -> Result<(), String> {
        let base_url = self.base_url().ok_or("no base URL configured")?;
        let _ = Self::verify_key_for(self.id, &base_url, key)?;
        self.reload();
        Ok(())
    }

    fn models(&self) -> Vec<ModelInfo> {
        if self.id == "custom" {
            let Some(base_url) = self.base_url() else {
                return Vec::new();
            };
            let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((at, models)) = cache.as_ref()
                && at.elapsed() < MODELS_TTL
            {
                return models.clone();
            }
            let models = self.fetch_custom_models(&base_url);
            *cache = Some((Instant::now(), models.clone()));
            return models;
        }

        let Some(key) = self.key().filter(|k| self.accepted(k)) else {
            return Vec::new();
        };
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((at, models)) = cache.as_ref()
            && at.elapsed() < MODELS_TTL
        {
            return models.clone();
        }
        match self.fetch_models_with_key(&key) {
            Ok(models) => {
                *cache = Some((Instant::now(), models.clone()));
                models
            }
            Err(e) => {
                tracing::warn!(error = %e, provider = self.id, "model list not updated");
                cache.as_ref().map(|(_, m)| m.clone()).unwrap_or_default()
            }
        }
    }

    fn can_resume(&self) -> bool {
        false
    }

    fn ask(&self, req: &AskRequest<'_>, cancel: &Cancel) -> Result<Answer, String> {
        let base_url = self.base_url().ok_or("no base URL configured")?;
        let key = self.key();
        if self.id == "openrouter" && key.is_none() {
            return Err("no API key".into());
        }
        let mut messages = Vec::new();
        if let Some(system) = req.system {
            messages.push(json!({ "role": "system", "content": system }));
        }
        messages.push(json!({ "role": "user", "content": req.prompt }));
        let mut body = json!({ "model": req.model, "temperature": 0, "messages": messages });
        if let Some(max) = req.max_tokens {
            body["max_tokens"] = json!(max);
        }
        let client = Self::client()?;
        let url = format!("{base_url}/chat/completions");
        // Temporary upstream failures (rate limits of free models) are retried, as in a task.
        let v = block_on(async {
            tokio::select! {
                r = tokio::time::timeout(req.timeout, chat(&client, &url, key.as_ref(), &body)) => {
                    r.map_err(|_| format!("no answer in {}s", req.timeout.as_secs()))?
                }
                () = cancelled(cancel) => Err("cancelled".into()),
            }
        })?
        .map_err(|e| {
            if let Some(k) = &key
                && (e.contains("Missing Authentication") || e.contains("User not found"))
            {
                return format!("{e} ({})", key_shape(k.expose()));
            }
            e
        })?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if text.trim().is_empty() {
            // Reasoning models may spend the whole limit on thinking and return no text.
            return Err("empty answer".into());
        }
        Ok(Answer {
            text,
            cost_usd: None,
        })
    }

    fn run(&self, req: &RunRequest<'_>, out: &mut dyn FnMut(Output), cancel: &Cancel) -> RunResult {
        let started = Instant::now();
        let mut tracker = Tracker::new(None);
        let mut emit = |ev: Event, tracker: &mut Tracker| {
            tracker.observe(&ev);
            out(Output::Event(ev));
        };
        emit(
            Event::Init {
                session: None,
                model: Some(req.model.to_owned()),
                cli: format!("Vibefork loop → {}", self.id),
                mcp: None,
                skills: None,
            },
            &mut tracker,
        );
        let Some(base_url) = self.base_url() else {
            emit(
                Event::Error {
                    text: "no base URL configured".into(),
                },
                &mut tracker,
            );
            return tracker.finish(1, false);
        };
        let key = self.key();
        if self.id == "openrouter" && key.is_none() {
            emit(
                Event::Error {
                    text: "no API key".into(),
                },
                &mut tracker,
            );
            return tracker.finish(1, false);
        }
        let client = match Self::client() {
            Ok(c) => c,
            Err(e) => {
                emit(Event::Error { text: e }, &mut tracker);
                return tracker.finish(1, false);
            }
        };
        let url = format!("{base_url}/chat/completions");
        let definitions = tools::definitions();
        let mut messages = vec![
            json!({ "role": "system", "content": system_prompt(req.system) }),
            json!({ "role": "user", "content": req.prompt }),
        ];
        let mut usage = Usage::default();
        let mut summary: Option<String> = None;

        for _ in 0..MAX_STEPS {
            if cancel.is_cancelled() {
                break;
            }
            let body = json!({ "model": req.model, "messages": messages, "tools": definitions, "tool_choice": "auto", "temperature": 0.2 });
            let response = block_on(async {
                tokio::select! {
                    r = chat(&client, &url, key.as_ref(), &body) => r.map(Some),
                    () = cancelled(cancel) => Ok(None),
                }
            });
            let data = match response {
                Ok(Ok(Some(v))) => v,
                Ok(Ok(None)) => break,
                Ok(Err(e)) | Err(e) => {
                    emit(
                        Event::Error {
                            text: format!("model request failed: {e}"),
                        },
                        &mut tracker,
                    );
                    break;
                }
            };
            usage.input_tokens += data["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
            usage.output_tokens += data["usage"]["completion_tokens"].as_u64().unwrap_or(0);
            let msg = &data["choices"][0]["message"];
            let content = msg["content"].as_str().unwrap_or_default().to_owned();
            let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            messages.push(
                json!({ "role": "assistant", "content": content, "tool_calls": msg["tool_calls"] }),
            );
            if !content.trim().is_empty() {
                emit(
                    Event::Text {
                        text: content.clone(),
                    },
                    &mut tracker,
                );
            }
            if calls.is_empty() {
                // An answer without tools is the final one.
                summary = Some(content);
                break;
            }
            for c in &calls {
                let name = c["function"]["name"].as_str().unwrap_or_default();
                let args: Value = c["function"]["arguments"]
                    .as_str()
                    .and_then(|a| serde_json::from_str(a).ok())
                    .unwrap_or(Value::Null);
                let detail = args["command"]
                    .as_str()
                    .or_else(|| args["path"].as_str())
                    .unwrap_or_default();
                emit(
                    Event::Tool {
                        tool: name.into(),
                        detail: detail.into(),
                    },
                    &mut tracker,
                );
                let result = match tools::call(req.dir, name, &args, req.env, cancel) {
                    Ok(Outcome::Done(text)) => {
                        emit(
                            Event::ToolResult {
                                error: false,
                                text: clip(&text, 2000),
                            },
                            &mut tracker,
                        );
                        text
                    }
                    Ok(Outcome::Finished(s)) => {
                        summary = Some(s);
                        "ok".into()
                    }
                    Err(e) => {
                        let text = format!("error: {e}");
                        emit(
                            Event::ToolResult {
                                error: true,
                                text: text.clone(),
                            },
                            &mut tracker,
                        );
                        text
                    }
                };
                messages
                    .push(json!({ "role": "tool", "tool_call_id": c["id"], "content": result }));
            }
            if summary.is_some() {
                break;
            }
        }
        let cancelled = cancel.is_cancelled();
        let text = summary.clone().unwrap_or_default();
        emit(
            Event::Result {
                ok: summary.is_some() && !cancelled,
                text,
                turns: None,
                ms: Some(started.elapsed().as_millis() as u64),
                cost_usd: None,
                usage: Some(usage),
            },
            &mut tracker,
        );
        tracker.finish(
            if summary.is_some() && !cancelled {
                0
            } else {
                1
            },
            cancelled,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_free_models_with_tools() {
        let body = json!({ "data": [
            { "id": "a/b:free", "supported_parameters": ["tools"] },
            { "id": "c/d", "pricing": { "prompt": "0", "completion": "0" }, "supported_parameters": ["tools", "x"] },
            { "id": "e/f:free", "supported_parameters": [] },
            { "id": "g/h", "pricing": { "prompt": "0.1", "completion": "0" }, "supported_parameters": ["tools"] },
        ]});
        let ids: Vec<String> = free_tool_models("openrouter", &body)
            .into_iter()
            .map(|m| m.model)
            .collect();
        assert_eq!(ids, ["a/b:free", "c/d"]);
    }

    #[test]
    fn gemini_models_parses_flash_models() {
        let body = json!({ "data": [
            { "id": "models/gemini-2.0-flash", "object": "model" },
            { "id": "models/gemini-1.5-flash", "object": "model" },
            { "id": "models/text-embedding-004", "object": "model" },
        ]});
        let models = gemini_models("gemini", &body);
        let ids: Vec<String> = models.into_iter().map(|m| m.model).collect();
        assert_eq!(ids, ["gemini-2.0-flash", "gemini-1.5-flash"]);
    }
}

/// Whether the API accepts the key (`GET /key` for OpenRouter, `GET /models` for others).
fn key_accepted(id: &str, base_url: &str, key: &Secret<String>) -> Result<bool, String> {
    let base = base_url.trim_end_matches('/');
    let url = if id == "openrouter" {
        format!("{base}/key")
    } else {
        format!("{base}/models")
    };
    block_on(async {
        let res = OpenAiCompatible::client()?
            .get(url)
            .bearer_auth(key.expose())
            .header("x-title", "vibefork")
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|_| format!("network error contacting {id}"))?;
        Ok(res.status().is_success())
    })?
}

/// What a rejected key looks like, without its value: length, the usual prefix, characters outside
/// printable ASCII. Enough to tell an empty, cut or mangled key from a revoked one.
fn key_shape(key: &str) -> String {
    let odd = key.chars().filter(|c| !c.is_ascii_graphic()).count();
    format!(
        "key: {} chars, sk-or- prefix: {}, unusual characters: {odd}",
        key.chars().count(),
        if key.starts_with("sk-or-") {
            "yes"
        } else {
            "no"
        }
    )
}

#[cfg(test)]
mod shape_tests {
    use super::key_shape;

    #[test]
    fn the_shape_never_shows_the_key() {
        let shape = key_shape("\u{feff}sk-or-v1-secretvalue");
        assert_eq!(
            shape,
            "key: 21 chars, sk-or- prefix: no, unusual characters: 1"
        );
        assert!(!shape.contains("secretvalue"));
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn an_empty_stored_key_is_no_key() {
        let p = OpenAiCompatible::dynamic("openrouter", OPENROUTER_URL, || {
            Some(Secret::new("  ".to_owned()))
        });
        assert!(p.key().is_none());
        assert!(p.models().is_empty());
        assert!(OpenAiCompatible::verify_key(OPENROUTER_URL, &Secret::new(String::new())).is_err());
    }
}

#[cfg(test)]
mod bom_tests {
    use super::*;

    #[test]
    fn a_stored_key_with_a_byte_order_mark_is_cleaned() {
        let p = OpenAiCompatible::dynamic("openrouter", OPENROUTER_URL, || {
            Some(Secret::new("\u{feff}sk-or-v1-abc\r\n".to_owned()))
        });
        assert_eq!(
            p.key().map(|k| k.expose().clone()).as_deref(),
            Some("sk-or-v1-abc")
        );
    }
}
