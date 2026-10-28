//! The session stream of a project: what the agent does, as events the editor draws on its canvas.
//!
//! Every event is numbered within the project ([`CanvasRecord::seq`], from 1, no gaps) and kept in
//! the project's event log, so the editor rebuilds the picture after a reload
//! (`GET /v1/canvas?project=…&from=N`) and can replay a session. Live events come over the WebSocket
//! as [`crate::ws::ServerMessage::Canvas`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Longest page of `GET /v1/canvas`.
pub const MAX_CANVAS_PAGE: usize = 2000;

/// A stage of the analysis (`download` … `summary`) or of a task (`understand` … `done`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Analysis: the project is being downloaded.
    Download,
    /// Analysis: what the project consists of.
    Parts,
    /// Analysis: how the parts are linked.
    Links,
    /// Analysis: tools for the build are being prepared.
    Tools,
    /// Analysis: what can be done with the project.
    Summary,
    /// Task: understanding the task.
    Understand,
    /// Task: making a plan.
    Plan,
    /// Task: changing the project.
    Change,
    /// Task: building and checking.
    Check,
    /// Task: finished.
    Done,
}

/// What an [`CanvasEvent::Eta`] estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EtaScope {
    /// The analysis of the project.
    Analysis,
    /// The running task.
    Task,
}

/// Why an offer is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfferSourceKind {
    /// Found in the project's code.
    Analysis,
    /// What people ask for in similar projects.
    Similar,
    /// What is asked in the project's issues.
    Issues,
}

/// Where an offer comes from, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OfferSource {
    /// Kind of source.
    pub kind: OfferSourceKind,
    /// The reason, in the person's words.
    pub text: String,
}

/// Something the person can do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Offer {
    /// Short title.
    pub title: String,
    /// What it means.
    pub text: String,
    /// Why it is offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<OfferSource>,
}

/// Kind of a result artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A build output.
    Build,
    /// A report of the checks.
    Report,
    /// A copy of the project before the change.
    Backup,
    /// The changes, file by file.
    Changes,
    /// The log of the work.
    Log,
}

/// A changed file inside a [`ArtifactKind::Changes`] artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactFile {
    /// Path inside the project.
    pub path: String,
    /// The part the file belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// What changed, in plain words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Changed lines, prefixed with `-` or `+`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diff: Vec<String>,
}

/// One check inside a [`ArtifactKind::Report`] artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactCheck {
    /// Whether it passed.
    pub ok: bool,
    /// What was checked.
    pub text: String,
    /// The part it is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

/// Something a task produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    /// Name shown to the person.
    pub name: String,
    /// Kind.
    pub kind: ArtifactKind,
    /// Size, in the person's words (`12 MB`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Where it lies, in the person's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// What it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// Parts it is about.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    /// Changed files ([`ArtifactKind::Changes`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ArtifactFile>,
    /// Checks ([`ArtifactKind::Report`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<ArtifactCheck>,
    /// Lines ([`ArtifactKind::Log`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
}

/// How a [`CanvasEvent::Result`] ended: the editor words it in the person's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    /// The task finished (exit code 0).
    Done,
    /// The task failed (`exit` is its code).
    Failed,
    /// The task was cancelled.
    Cancelled,
    /// One part was rolled back (`id` is the part).
    PartRolledBack,
}

/// What a [`CanvasEvent::Check`] checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    /// The project's build (`exit` is its code).
    Build,
}

/// What the agent does, as the canvas shows it. Tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanvasEvent {
    /// The agent enters a stage.
    Stage {
        /// The stage.
        id: Stage,
    },
    /// Progress of the current stage (download, tool install, build). Without `unit` the amount
    /// is unknown: `total` and `eta` are 0 and an editor shows that the work goes on.
    Progress {
        /// Done so far, in `unit`.
        done: u64,
        /// Total, in `unit`; 0 without `unit`.
        total: u64,
        /// Unit: `bytes`, `files`, `steps`…; none when the amount is unknown.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        /// Seconds left; 0 without `unit`.
        eta: u64,
        /// What is being done.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// The map node this work belongs to: the root (`root`) while the project downloads, the
        /// node of the build (the root, as the map has no node of its own for it) while tools are
        /// prepared and the project is built.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// The tool being prepared (`java@temurin-21`), while tools are prepared.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spec: Option<String>,
    },
    /// Overall time left, by the measured durations of earlier stages and steps; sent again as
    /// the work enters each stage and step.
    Eta {
        /// What is estimated.
        scope: EtaScope,
        /// Seconds left.
        seconds: u64,
    },
    /// The agent waits for the AI: what it is thinking about, in plain words.
    Think {
        /// The part it is about.
        id: String,
        /// The thought.
        text: String,
    },
    /// The root of the map.
    Project {
        /// Node id.
        id: String,
        /// Name.
        label: String,
        /// What it is.
        summary: String,
        /// Another version (a fork) of this project: the project it is a version of.
        #[serde(rename = "versionOf", default, skip_serializing_if = "Option::is_none")]
        version_of: Option<String>,
        /// Files of the node.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
    },
    /// A part of the project was found.
    Found {
        /// Node id.
        id: String,
        /// Parent node id.
        parent: String,
        /// Name.
        label: String,
        /// What it is.
        summary: String,
        /// Files of the part.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
    },
    /// A smaller piece inside a part.
    Subsystem {
        /// Node id.
        id: String,
        /// Parent node id.
        parent: String,
        /// Name.
        label: String,
        /// What it is.
        summary: String,
        /// Files of the piece.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
    },
    /// The agent looks at a part now.
    Focus {
        /// Node id.
        id: String,
    },
    /// The agent moves from one part to another.
    Touch {
        /// Node id it leaves.
        from: String,
        /// Node id it enters.
        to: String,
    },
    /// What the agent understood about a part, in plain words.
    Insight {
        /// Node id.
        id: String,
        /// The insight.
        text: String,
    },
    /// What the person can do next, offered when the analysis ends.
    Offers {
        /// The offers.
        items: Vec<Offer>,
    },
    /// The task the person gave.
    Task {
        /// The task, in the person's words.
        text: String,
        /// Parts it is about.
        nodes: Vec<String>,
    },
    /// The plan of the task.
    Plan {
        /// Step titles.
        steps: Vec<String>,
    },
    /// The agent starts a plan step.
    Step {
        /// Step index, from 0.
        index: u32,
    },
    /// The agent changed a part.
    Change {
        /// Node id.
        id: String,
        /// Changed files of the part.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<String>,
    },
    /// A build or a check finished.
    Check {
        /// Whether it passed.
        ok: bool,
        /// What happened, in plain English; an editor words it itself by `kind` when given.
        text: String,
        /// The part where a failed check broke.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// What was checked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<CheckKind>,
        /// Exit code of the check's process.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit: Option<i32>,
    },
    /// The person paused the task; the interrupted step runs again on resume.
    Paused,
    /// The paused task goes on.
    Resumed,
    /// The agent took a message of the person into account.
    #[serde(rename = "note.taken")]
    NoteTaken {
        /// The message.
        #[serde(rename = "noteId")]
        note_id: String,
        /// Parts it is about.
        related: Vec<String>,
        /// The agent's answer.
        reply: String,
    },
    /// The task ended.
    Result {
        /// The outcome, in plain English; an editor words it itself by `kind` when given.
        text: String,
        /// Details, in plain English (for `done` and `failed`: the changed files' summary).
        detail: String,
        /// What the task produced.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        artifacts: Vec<Artifact>,
        /// How it ended.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<ResultKind>,
        /// Exit code of the task (`done` and `failed`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit: Option<i32>,
        /// The part it is about (`part_rolled_back`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
}

/// A numbered event of a project's stream, as kept in its event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanvasRecord {
    /// Number within the project, from 1, without gaps.
    pub seq: u64,
    /// When, Unix time in milliseconds.
    pub at: u64,
    /// The run it belongs to (a task, a build, an analysis).
    pub session: String,
    /// The event.
    pub ev: CanvasEvent,
}

/// Response of `GET /v1/canvas?project=…&from=N[&session=…]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CanvasResponse {
    /// Events with `seq ≥ from`, oldest first, at most [`MAX_CANVAS_PAGE`].
    pub events: Vec<CanvasRecord>,
    /// `from` for the next page: one past the last returned event (or `from` when none).
    pub next: u64,
}

/// A file or text the person attaches to a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NoteAttachment {
    /// File name, or a short label for pasted text.
    pub name: String,
    /// MIME type (`text/plain`, `image/png`…).
    pub mime: String,
    /// Text as is, binary data as base64.
    pub data: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(ev: &CanvasEvent) -> serde_json::Value {
        serde_json::to_value(ev).unwrap()
    }

    #[test]
    fn events_have_the_wire_names_of_the_editor() {
        assert_eq!(
            wire(&CanvasEvent::Stage {
                id: Stage::Understand
            }),
            serde_json::json!({"type": "stage", "id": "understand"})
        );
        assert_eq!(
            wire(&CanvasEvent::Project {
                id: "root".into(),
                label: "notes-app (fork)".into(),
                summary: "Another version with tags".into(),
                version_of: Some("upstream".into()),
                files: Vec::new(),
            }),
            serde_json::json!({"type": "project", "id": "root", "label": "notes-app (fork)", "summary": "Another version with tags", "versionOf": "upstream"})
        );
        assert_eq!(
            wire(&CanvasEvent::NoteTaken {
                note_id: "n1".into(),
                related: vec!["a".into()],
                reply: "ok".into()
            }),
            serde_json::json!({"type": "note.taken", "noteId": "n1", "related": ["a"], "reply": "ok"})
        );
        assert_eq!(
            wire(&CanvasEvent::Eta {
                scope: EtaScope::Analysis,
                seconds: 90
            }),
            serde_json::json!({"type": "eta", "scope": "analysis", "seconds": 90})
        );
        assert_eq!(
            wire(&CanvasEvent::Progress {
                done: 0,
                total: 0,
                unit: None,
                eta: 0,
                text: None,
                id: Some("root".into()),
                spec: Some("go@1.23".into()),
            }),
            serde_json::json!({"type": "progress", "done": 0, "total": 0, "eta": 0, "id": "root", "spec": "go@1.23"})
        );
    }

    #[test]
    fn a_recorded_session_reads_back() {
        let lines = [
            r#"{"seq":1,"at":1000,"session":"s1","ev":{"type":"task","text":"Add a dark theme","nodes":["ui"]}}"#,
            r#"{"seq":2,"at":1500,"session":"s1","ev":{"type":"plan","steps":["Read","Fix"]}}"#,
            r#"{"seq":3,"at":2000,"session":"s1","ev":{"type":"step","index":0}}"#,
            r#"{"seq":4,"at":2500,"session":"s1","ev":{"type":"progress","done":3,"total":10,"unit":"files","eta":12,"id":"ui"}}"#,
            r#"{"seq":5,"at":3000,"session":"s1","ev":{"type":"offers","items":[{"title":"Export to PDF","text":"Save a note as a PDF","source":{"kind":"issues","text":"Asked for in 4 issues"}}]}}"#,
            r#"{"seq":6,"at":3500,"session":"s1","ev":{"type":"check","ok":false,"text":"Build failed","id":"ui"}}"#,
            r#"{"seq":7,"at":4000,"session":"s1","ev":{"type":"result","text":"Done","detail":"2 files","artifacts":[{"name":"notes-app.zip","kind":"build","size":"2 MB"},{"name":"Changes","kind":"changes","files":[{"path":"src/ui/theme.ts","node":"ui","diff":["-light","+dark"]}]}]}}"#,
        ];
        for line in lines {
            let record: CanvasRecord = serde_json::from_str(line).unwrap();
            let back = serde_json::to_value(&record).unwrap();
            assert_eq!(
                back,
                serde_json::from_str::<serde_json::Value>(line).unwrap(),
                "{line}"
            );
        }
    }

    #[test]
    fn unknown_event_types_are_rejected() {
        assert!(serde_json::from_str::<CanvasEvent>(r#"{"type":"explode"}"#).is_err());
    }
}
