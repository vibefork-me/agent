//! The agent's own coding loop over an OpenAI-compatible chat API (OpenRouter): the model gets file
//! and shell tools limited to the project folder. Works with nothing installed but an API key.

use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use vf_core::Secret;
use vf_protocol::{Cancel, CostClass, Event, ModelInfo, Usage};

use crate::tools::{self, Outcome};
use crate::{Answer, AskRequest, Output, Provider, RunRequest, RunResult, Tracker, clip};

/// OpenRouter API.
pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
/// Most model turns of one run.
pub const MAX_STEPS: usize = 30;
/// How long the model list is cached.
const MODELS_TTL: Duration = Duration::from_secs(600);
/// Timeout of one model request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// An OpenAI-compatible API with a key.
pub struct OpenAiCompatible {
    id: &'static str,
    base_url: String,
    key_source: Box<dyn Fn() -> Option<Secret<String>> + Send + Sync>,
    cache: Mutex<Option<(Instant, Vec<ModelInfo>)>>,
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
            base_url: base_url.trim_end_matches('/').to_owned(),
            key_source: Box::new(key_source),
            cache: Mutex::new(None),
        }
    }

    /// The API key, if present.
    pub fn key(&self) -> Option<Secret<String>> {
        (self.key_source)()
    }

    /// Clears any cached model list.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        *cache = None;
    }

    fn client() -> Result<reqwest::Client, String> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| e.to_string())
    }

    /// Validates an API key by fetching models from `base_url` with Bearer auth.
    pub fn verify_key(base_url: &str, key: &Secret<String>) -> Result<Vec<ModelInfo>, String> {
        let url = format!("{}/models", base_url.trim_end_matches('/'));
        let body: Value = block_on(async {
            let res = Self::client()?
                .get(&url)
                .bearer_auth(key.expose())
                .header("x-title", "vibefork")
                .send()
                .await
                .map_err(|_| "network error contacting OpenRouter".to_owned())?;
            let status = res.status();
            if !status.is_success() {
                return Err(format!("OpenRouter returned HTTP {status}"));
            }
            res.json::<Value>()
                .await
                .map_err(|_| "invalid JSON response from OpenRouter".to_owned())
        })??;
        Ok(free_tool_models("openrouter", &body))
    }

    fn fetch_models_with_key(&self, key: &Secret<String>) -> Result<Vec<ModelInfo>, String> {
        let url = format!("{}/models", self.base_url);
        let body: Value = block_on(async {
            let res = Self::client()?
                .get(&url)
                .bearer_auth(key.expose())
                .header("x-title", "vibefork")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = res.status();
            if !status.is_success() {
                return Err(format!("OpenRouter returned HTTP {status}"));
            }
            res.json::<Value>().await.map_err(|e| e.to_string())
        })??;
        Ok(free_tool_models(self.id, &body))
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

/// One chat request; temporary failures (rate limit, upstream errors) are retried twice.
async fn chat(
    client: &reqwest::Client,
    url: &str,
    key: &Secret<String>,
    body: &Value,
) -> Result<Value, String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = async {
            let res = client
                .post(url)
                .bearer_auth(key.expose())
                .header("x-title", "vibefork")
                .json(body)
                .send()
                .await
                .map_err(|e| (true, e.to_string()))?;
            let status = res.status();
            let v: Value = res.json().await.map_err(|e| (true, e.to_string()))?;
            if !status.is_success() || !v["error"].is_null() {
                let message = v["error"]["message"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("HTTP {status}"));
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
        let has_key = self.key().is_some();
        crate::Status {
            installed: true,
            signed_in: Some(has_key),
        }
    }

    fn reload(&self) {
        self.clear_cache();
    }

    fn connect_key(&self, key: &Secret<String>) -> Result<(), String> {
        let _ = Self::verify_key(&self.base_url, key)?;
        self.reload();
        Ok(())
    }

    fn models(&self) -> Vec<ModelInfo> {
        let Some(key) = self.key() else {
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
        let key = self.key().ok_or("no API key")?;
        let mut messages = Vec::new();
        if let Some(system) = req.system {
            messages.push(json!({ "role": "system", "content": system }));
        }
        messages.push(json!({ "role": "user", "content": req.prompt }));
        let mut body = json!({ "model": req.model, "temperature": 0, "messages": messages });
        if let Some(max) = req.max_tokens {
            body["max_tokens"] = json!(max);
        }
        let request = Self::client()?
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(key.expose())
            .header("x-title", "vibefork")
            .timeout(req.timeout)
            .json(&body)
            .send();
        let v = block_on(async {
            tokio::select! {
                r = async {
                    let v: Value = request.await.map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
                    Ok::<Value, String>(v)
                } => r,
                () = cancelled(cancel) => Err("cancelled".into()),
            }
        })??;
        if let Some(e) = v["error"]["message"].as_str() {
            return Err(e.to_owned());
        }
        Ok(Answer {
            text: v["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
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
        let Some(key) = self.key() else {
            emit(
                Event::Error {
                    text: "no API key".into(),
                },
                &mut tracker,
            );
            return tracker.finish(1, false);
        };
        let client = match Self::client() {
            Ok(c) => c,
            Err(e) => {
                emit(Event::Error { text: e }, &mut tracker);
                return tracker.finish(1, false);
            }
        };
        let url = format!("{}/chat/completions", self.base_url);
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
                    r = chat(&client, &url, &key, &body) => r.map(Some),
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
}
