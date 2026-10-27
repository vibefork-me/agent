//! Generated API description in `schema/` (repository root):
//! - `protocol.schema.json` — JSON Schema of every protocol type;
//! - `protocol.ts` — TypeScript types for the web UI, generated from that JSON Schema;
//! - `openapi.json` — OpenAPI 3.1 of the HTTP routes (`vf_server::ROUTES`).
//!
//! The test fails when the files are out of date. Update: `VF_UPDATE_SCHEMA=1 cargo test -p vf-server --test schema`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::path::PathBuf;

use schemars::r#gen::SchemaSettings;
use serde_json::{Map, Value, json};
use vf_protocol::canvas::{CanvasRecord, CanvasResponse};
use vf_protocol::http::*;
use vf_protocol::ws::{ClientMessage, ServerMessage};

fn definitions() -> Map<String, Value> {
    let mut g = SchemaSettings::draft07()
        .with(|s| s.definitions_path = "#/components/schemas/".into())
        .into_generator();
    macro_rules! add {
        ($($t:ty),* $(,)?) => { $( g.subschema_for::<$t>(); )* };
    }
    add!(
        Hello,
        ErrorBody,
        PairStartRequest,
        PairStartResponse,
        PairStatusResponse,
        ChallengeRequest,
        ChallengeResponse,
        SessionRequest,
        SessionResponse,
        Me,
        PairInfo,
        RevokeRequest,
        RevokeResponse,
        ProjectsResponse,
        ProjectInfo,
        ModelsResponse,
        AiResponse,
        AiProvider,
        AiKind,
        AiConnected,
        CliHomeMode,
        AiConnectRequest,
        AiConnectResponse,
        AiDisconnectRequest,
        AiDisconnectResponse,
        JournalResponse,
        RollbackRequest,
        RollbackResponse,
        GithubConnectResponse,
        GithubStatusResponse,
        ImportRequest,
        ImportResponse,
        ForeignChanges,
        SaveRequest,
        SaveResponse,
        FileResponse,
        ProjectFilesResponse,
        FileDiffResponse,
        GraphResponse,
        GraphSource,
        DescribeRequest,
        GraphNode,
        GraphEdge,
        CanvasResponse,
        CanvasRecord,
        ClientMessage,
        ServerMessage,
    );
    let defs = serde_json::to_value(g.definitions()).unwrap();
    defs.as_object().cloned().unwrap()
}

// ---------- TypeScript from JSON Schema (the subset schemars produces for our types) ----------

fn ts(schema: &Value) -> String {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        return r.rsplit('/').next().unwrap().to_owned();
    }
    if let Some(all) = schema.get("allOf").and_then(Value::as_array)
        && all.len() == 1
    {
        return ts(&all[0]);
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(variants) = schema.get(key).and_then(Value::as_array) {
            return variants.iter().map(ts).collect::<Vec<_>>().join(" | ");
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if types.is_empty() {
        return "unknown".into();
    }
    types
        .iter()
        .map(|t| match *t {
            "string" => "string".into(),
            "integer" | "number" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => format!(
                "({})[]",
                schema
                    .get("items")
                    .map(ts)
                    .unwrap_or_else(|| "unknown".into())
            ),
            "object" => object(schema),
            other => panic!("unsupported JSON Schema type {other}"),
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn object(schema: &Value) -> String {
    let Some(props) = schema.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".into();
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let fields: Vec<String> = props
        .iter()
        .map(|(name, s)| {
            format!(
                "{name}{}: {}",
                if required.contains(&name.as_str()) {
                    ""
                } else {
                    "?"
                },
                ts(s)
            )
        })
        .collect();
    format!("{{ {} }}", fields.join("; "))
}

fn typescript(defs: &Map<String, Value>) -> String {
    let mut out = String::from(
        "// Generated from protocol.schema.json by vf-server/tests/schema.rs. Do not edit.\n\n",
    );
    for (name, schema) in defs {
        if let Some(doc) = schema.get("description").and_then(Value::as_str) {
            out.push_str(&format!("/** {} */\n", doc.replace('\n', " ")));
        }
        out.push_str(&format!("export type {name} = {};\n\n", ts(schema)));
    }
    out
}

// ---------- OpenAPI ----------

fn openapi(defs: &Map<String, Value>) -> Value {
    let reference = |name: &str| json!({ "$ref": format!("#/components/schemas/{name}") });
    let mut paths = Map::new();
    for r in vf_server::ROUTES {
        let mut op = json!({
            "summary": r.summary,
            "responses": {
                "200": match r.response {
                    Some(name) if r.path == "/v1/pairs" => json!({ "description": "OK", "content": { "application/json": { "schema": { "type": "array", "items": reference(name) } } } }),
                    Some(name) => json!({ "description": "OK", "content": { "application/json": { "schema": reference(name) } } }),
                    None => json!({ "description": "OK" }),
                },
                "default": { "description": "Error", "content": { "application/json": { "schema": reference("ErrorBody") } } }
            }
        });
        if let Some(name) = r.request {
            op["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": reference(name) } } });
        }
        if r.session {
            op["security"] = json!([{ "session": [] }]);
        }
        let entry = paths.entry(r.path.to_owned()).or_insert_with(|| json!({}));
        entry[r.method.to_lowercase()] = op;
    }
    json!({
        "openapi": "3.1.0",
        "info": { "title": "vibefork agent", "version": vf_protocol::API_VERSION, "description": "Local API on 127.0.0.1. Every POST needs `x-vibefork: 1` and JSON. WebSocket /v1/ws: see ClientMessage / ServerMessage." },
        "paths": paths,
        "components": {
            "schemas": defs,
            "securitySchemes": { "session": { "type": "http", "scheme": "bearer", "description": "Token from POST /v1/auth/session" } }
        }
    })
}

#[test]
fn generated_schema_is_up_to_date() {
    let defs = definitions();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema");
    let files = [
        ("protocol.schema.json", format!("{}\n", serde_json::to_string_pretty(&json!({ "$schema": "http://json-schema.org/draft-07/schema#", "definitions": defs })).unwrap())),
        ("protocol.ts", typescript(&defs)),
        ("openapi.json", format!("{}\n", serde_json::to_string_pretty(&openapi(&defs)).unwrap())),
    ];
    if std::env::var("VF_UPDATE_SCHEMA").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in &files {
            std::fs::write(dir.join(name), content).unwrap();
        }
        return;
    }
    for (name, content) in &files {
        let on_disk = std::fs::read_to_string(dir.join(name))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            on_disk == *content,
            "schema/{name} is out of date: run `VF_UPDATE_SCHEMA=1 cargo test -p vf-server --test schema`"
        );
    }
}
