//! `/v1/ws`: Origin is checked by the guard on upgrade; the first message must authenticate the
//! connection (5 s). A plain `ping` before authentication is answered with `echo: ping`
//! (connectivity check without a session).
//!
//! After authentication: `task` starts a task (one at a time per connection), `cancel` cancels it,
//! `stop` pauses it, `resume` continues it, cancelling a paused task undoes its changes; `note`
//! queues a message for the running task of that project (checked against the bounds of
//! `vf_protocol::note`; otherwise an error); `rollback_part` rolls one part back after the native
//! dialog confirms it (not while a task of the connection runs); `choose_offer` runs a task with
//! the title and text of an offer kept for the project (after the same access check as `task`; an
//! offer that is not there, or has another title than the one shown, is an error `no_offers` /
//! `bad_index`); `explain` answers a question about the project or one of its parts in words, after
//! the same access check as `task` and without a native dialog, as it changes nothing (one at a time
//! per connection, beside a running task of another project), other text is echoed. Messages are at most [`MAX_MESSAGE`] bytes. A running task also sends its session stream (`canvas` messages). Closing the connection cancels the running task.
//!
//! The first task (`task` or `choose_offer`) a paired browser gives in a project starts only after
//! the person answers Start in the native dialog, which shows the project, the task as given and
//! what the model may do (invariant 3); refused, it is the error `start_not_confirmed`. Further
//! tasks of that browser in the project start without asking while it keeps a connection to the
//! agent open (the editor keeps one while it is open); when its last connection closes, the agent
//! forgets it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, State};
use axum::response::Response;
use tokio::sync::mpsc;
use vf_protocol::note::NoteRejected;
use vf_protocol::ws::{CLOSE_AUTH_FAILED, ClientMessage, ServerMessage};
use vf_protocol::{Backend, Cancel, ExplainRequest, OfferRejected, TaskRequest};

use crate::AppState;
use crate::guard::EffectiveOrigin;

/// Time a connection has to authenticate.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest WebSocket message, bytes: a `note` with its attachments fits.
const MAX_MESSAGE: usize = vf_protocol::note::MAX_NOTE_DATA + 1024 * 1024;

/// Projects where a paired browser started work (by key id), kept while the browser has an open
/// connection.
#[derive(Debug, Default)]
pub(crate) struct StartedWork(Mutex<HashMap<String, (usize, HashSet<String>)>>);

impl StartedWork {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (usize, HashSet<String>)>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A connection of `key_id` authenticated; the returned guard counts it until dropped.
    fn open(self: &Arc<Self>, key_id: &str) -> Present {
        self.lock().entry(key_id.to_owned()).or_default().0 += 1;
        Present {
            work: self.clone(),
            key_id: key_id.to_owned(),
        }
    }

    /// Whether `key_id` started work in `project`.
    fn started(&self, key_id: &str, project: &str) -> bool {
        self.lock()
            .get(key_id)
            .is_some_and(|(_, projects)| projects.contains(project))
    }

    /// Remembers that `key_id` started work in `project`, while one of its connections is open.
    fn start(&self, key_id: &str, project: &str) {
        if let Some((_, projects)) = self.lock().get_mut(key_id) {
            projects.insert(project.to_owned());
        }
    }
}

/// An open, authenticated connection of a paired browser; the last one closing forgets where the
/// browser started work.
struct Present {
    work: Arc<StartedWork>,
    key_id: String,
}

impl Drop for Present {
    fn drop(&mut self) {
        let mut all = self.work.lock();
        if let Some(entry) = all.get_mut(&self.key_id) {
            entry.0 = entry.0.saturating_sub(1);
            if entry.0 == 0 {
                all.remove(&self.key_id);
            }
        }
    }
}

/// What runs on a connection: a task of a project (takes messages of the person) or a build.
struct Running {
    cancel: Cancel,
    /// Project of a task; `None` for a build.
    task_of: Option<String>,
}

pub(crate) async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(origin): Extension<EffectiveOrigin>,
) -> Response {
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| connection(socket, state, origin.0))
}

async fn connection(mut socket: WebSocket, state: AppState, origin: String) {
    let mut authed: Option<String> = None;
    let mut _present: Option<Present> = None;
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let mut running: Option<Running> = None;
    // An explanation under way: it changes nothing, so it does not take the place of a task.
    let mut explaining: Option<Cancel> = None;
    loop {
        let next = tokio::select! {
            out = rx.recv() => {
                match out {
                    Some(Out::Message(m)) => {
                        if send(&mut socket, &m).await.is_err() {
                            break;
                        }
                    }
                    Some(Out::Finished) | None => running = None,
                    Some(Out::Explained) => explaining = None,
                }
                continue;
            }
            next = recv(&mut socket, authed.is_none().then_some(deadline)) => match next {
                Some(m) => m,
                None => return close(socket, "auth timeout").await,
            },
        };
        let Some(Ok(message)) = next else { break };
        let Message::Text(text) = message else {
            continue;
        };
        let text = text.as_str();

        if authed.is_none() {
            if text == "ping" {
                if socket
                    .send(Message::Text("echo: ping".into()))
                    .await
                    .is_err()
                {
                    return;
                }
                continue;
            }
            let session = match serde_json::from_str::<ClientMessage>(text) {
                Ok(ClientMessage::Auth { token }) => state.auth.session(&token, &origin),
                Ok(_) | Err(_) => None, // anything before authentication fails it
            };
            match session {
                Some(s) => {
                    _present = Some(state.started.open(&s.key_id));
                    authed = Some(s.key_id.clone());
                    if send(&mut socket, &ServerMessage::Authed { key_id: s.key_id })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                None => {
                    let _ = send(
                        &mut socket,
                        &ServerMessage::Error {
                            error: "auth failed".into(),
                            code: Some("auth_failed".into()),
                        },
                    )
                    .await;
                    return close(socket, "auth failed").await;
                }
            }
            continue;
        }

        let reply = match serde_json::from_str::<ClientMessage>(text) {
            Ok(ClientMessage::Task {
                project,
                prompt,
                provider,
                model,
                continue_,
                lang,
            }) => {
                if running.is_some() {
                    Some(ServerMessage::Error {
                        error: "a task is already running".into(),
                        code: Some("task_already_running".into()),
                    })
                } else {
                    let task = TaskRequest {
                        project,
                        prompt,
                        provider,
                        model,
                        continue_,
                        lang,
                    };
                    let key = authed.clone().unwrap_or_default();
                    let task_of = Some(task.project.clone());
                    let cancel = start(
                        &state,
                        origin.clone(),
                        key,
                        task.project.clone(),
                        move |_| Ok(task),
                        tx.clone(),
                    );
                    running = Some(Running { cancel, task_of });
                    None
                }
            }
            Ok(ClientMessage::ChooseOffer {
                project,
                index,
                title,
                provider,
                model,
                continue_,
                lang,
            }) => {
                if running.is_some() {
                    Some(ServerMessage::Error {
                        error: "a task is already running".into(),
                        code: Some("task_already_running".into()),
                    })
                } else {
                    let task = TaskRequest {
                        project,
                        prompt: String::new(),
                        provider,
                        model,
                        continue_,
                        lang,
                    };
                    let key = authed.clone().unwrap_or_default();
                    let task_of = Some(task.project.clone());
                    let cancel = start(
                        &state,
                        origin.clone(),
                        key,
                        task.project.clone(),
                        move |backend| {
                            backend
                                .offer_prompt(&task.project, index, title.as_deref())
                                .map(|prompt| TaskRequest { prompt, ..task })
                        },
                        tx.clone(),
                    );
                    running = Some(Running { cancel, task_of });
                    None
                }
            }
            Ok(ClientMessage::Build { project }) => {
                if running.is_some() {
                    Some(ServerMessage::Error {
                        error: "a task is already running".into(),
                        code: Some("task_already_running".into()),
                    })
                } else {
                    let key = authed.clone().unwrap_or_default();
                    let cancel = start_confirmed(
                        &state,
                        Confirmed::Build,
                        origin.clone(),
                        key,
                        project,
                        tx.clone(),
                    );
                    running = Some(Running {
                        cancel,
                        task_of: None,
                    });
                    None
                }
            }
            Ok(ClientMessage::Run { project }) => {
                if running.is_some() {
                    Some(ServerMessage::Error {
                        error: "a task is already running".into(),
                        code: Some("task_already_running".into()),
                    })
                } else {
                    let key = authed.clone().unwrap_or_default();
                    let cancel = start_confirmed(
                        &state,
                        Confirmed::Run,
                        origin.clone(),
                        key,
                        project,
                        tx.clone(),
                    );
                    running = Some(Running {
                        cancel,
                        task_of: None,
                    });
                    None
                }
            }
            Ok(ClientMessage::Cancel) => {
                // Cancelling a paused task is the person's "cancel the task": its changes go.
                match running.as_ref().map(|r| &r.cancel) {
                    Some(c) if c.is_paused() => c.cancel_and_undo(),
                    Some(c) => c.cancel(),
                    None => {}
                }
                None
            }
            Ok(ClientMessage::Stop) => {
                if let Some(r) = &running {
                    r.cancel.pause();
                }
                None
            }
            Ok(ClientMessage::Resume) => {
                if let Some(r) = &running {
                    r.cancel.resume();
                }
                None
            }
            Ok(ClientMessage::RollbackPart { project, id }) => {
                if running.is_some() {
                    Some(ServerMessage::Error {
                        error: "a task is already running".into(),
                        code: Some("task_already_running".into()),
                    })
                } else {
                    let key = authed.clone().unwrap_or_default();
                    rollback_part(&state, origin.clone(), key, project, id, tx.clone());
                    None
                }
            }
            Ok(ClientMessage::Explain {
                project,
                question,
                part,
                provider,
                model,
                lang,
            }) => {
                if explaining.is_some() {
                    Some(ServerMessage::Error {
                        error: "an explanation is already under way".into(),
                        code: Some("explain_already_running".into()),
                    })
                } else {
                    let key = authed.clone().unwrap_or_default();
                    let req = ExplainRequest {
                        project,
                        question,
                        part,
                        provider,
                        model,
                        lang,
                    };
                    explaining = Some(explain(&state, origin.clone(), key, req, tx.clone()));
                    None
                }
            }
            Ok(ClientMessage::Note {
                project,
                text,
                attachments,
            }) => note(running.as_ref(), &project, text, attachments),
            Ok(ClientMessage::Auth { .. }) => None,
            Err(_) => Some(ServerMessage::Echo {
                data: text.to_owned(),
            }),
        };
        if let Some(reply) = reply
            && send(&mut socket, &reply).await.is_err()
        {
            break;
        }
    }
    // The browser left: a running task and an explanation are cancelled.
    if let Some(r) = running {
        r.cancel.cancel();
    }
    if let Some(c) = explaining {
        c.cancel();
    }
}

/// Queues a message of the person for the running task of `project`; the answer is an error, or
/// nothing (`note.taken` comes in the session stream when the task takes it).
fn note(
    running: Option<&Running>,
    project: &str,
    text: String,
    attachments: Vec<vf_protocol::canvas::NoteAttachment>,
) -> Option<ServerMessage> {
    let Some(cancel) = running
        .filter(|r| r.task_of.as_deref() == Some(project))
        .map(|r| &r.cancel)
    else {
        return Some(ServerMessage::Error {
            error:
                "no task is running in this project: a message can be sent only while a task runs"
                    .into(),
            code: Some("no_task".into()),
        });
    };
    match cancel.post_note(text, attachments) {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            error: e.to_string(),
            code: Some(
                match e {
                    NoteRejected::NotRunning => "no_task",
                    _ => "note_rejected",
                }
                .into(),
            ),
        }),
    }
}

/// A project action that runs its code, so it is confirmed in the native dialog (invariant 3).
#[derive(Debug, Clone, Copy)]
enum Confirmed {
    /// Build the project.
    Build,
    /// Run the project's program.
    Run,
}

/// Builds or runs after the user confirms it in the native dialog (invariant 3).
fn start_confirmed(
    state: &AppState,
    action: Confirmed,
    origin: String,
    key_id: String,
    project: String,
    tx: mpsc::UnboundedSender<Out>,
) -> Cancel {
    let cancel = Cancel::new();
    let (backend, auth, c) = (state.backend.clone(), state.auth.clone(), cancel.clone());
    tokio::task::spawn_blocking(move || {
        let lang = auth.dialog().language();
        let (refused, code) = match action {
            Confirmed::Build => ("build not confirmed", "build_not_confirmed"),
            Confirmed::Run => ("run not confirmed", "run_not_confirmed"),
        };
        // What the dialog shows: the action, why, and the exact commands it starts.
        let request = || {
            let ((what, why), commands) = match action {
                Confirmed::Build => (
                    vf_core::catalog::permission_build(lang, &project),
                    backend.build_commands(&project),
                ),
                Confirmed::Run => (
                    vf_core::catalog::permission_run(lang, &project),
                    backend.run_command(&project).into_iter().collect(),
                ),
            };
            vf_core::Permission::new(what, why).with_commands(commands)
        };
        if auth.project_allowed(&key_id, &origin, &project)
            && auth.ask_permission(&origin, &request())
        {
            let mut out = |m| {
                let _ = tx.send(Out::Message(m));
            };
            match action {
                Confirmed::Build => backend.build(&project, &mut out, &c),
                Confirmed::Run => backend.run_program(&project, &mut out, &c),
            }
        } else {
            let _ = tx.send(Out::Message(ServerMessage::Error {
                error: refused.into(),
                code: Some(code.into()),
            }));
        }
        let _ = tx.send(Out::Finished);
    });
    cancel
}

/// Rolls one part back after the user confirms it in the native dialog (invariant 3); the dialog
/// names the part as the map shows it. Its messages go to `tx`; the engine refuses while a task
/// runs in the project.
fn rollback_part(
    state: &AppState,
    origin: String,
    key_id: String,
    project: String,
    id: String,
    tx: mpsc::UnboundedSender<Out>,
) {
    let (backend, auth) = (state.backend.clone(), state.auth.clone());
    tokio::task::spawn_blocking(move || {
        let mut out = |m| {
            let _ = tx.send(Out::Message(m));
        };
        if !auth.project_allowed(&key_id, &origin, &project) {
            return out(ServerMessage::Error {
                error: "no access to this project".into(),
                code: Some("project_forbidden".into()),
            });
        }
        let lang = auth.dialog().language();
        let part = backend
            .graph(&project, false)
            .ok()
            .and_then(|g| g.nodes.into_iter().find(|n| n.id == id))
            .map(|n| n.label)
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| id.clone());
        let (what, why) = vf_core::catalog::permission_rollback_part(lang, &project, &part);
        if auth.ask_permission(&origin, &vf_core::Permission::new(what, why)) {
            backend.rollback_part(&project, &id, &mut out);
        } else {
            out(ServerMessage::Error {
                error: "rollback not confirmed".into(),
                code: Some("rollback_not_confirmed".into()),
            });
        }
    });
}

/// Answers a question in words on a blocking thread; its messages go to `tx`. The browser needs
/// access to the project (native dialog the first time); nothing else is asked, as nothing changes.
fn explain(
    state: &AppState,
    origin: String,
    key_id: String,
    req: ExplainRequest,
    tx: mpsc::UnboundedSender<Out>,
) -> Cancel {
    let cancel = Cancel::new();
    let (backend, auth, c) = (state.backend.clone(), state.auth.clone(), cancel.clone());
    tokio::task::spawn_blocking(move || {
        let mut out = |m| {
            let _ = tx.send(Out::Message(m));
        };
        if auth.project_allowed(&key_id, &origin, &req.project) {
            backend.explain(req, &mut out, &c);
        } else {
            out(ServerMessage::Error {
                error: "no access to this project".into(),
                code: Some("project_forbidden".into()),
            });
        }
        let _ = tx.send(Out::Explained);
    });
    cancel
}

/// The next message; `None` when `deadline` passed first.
async fn recv(
    socket: &mut WebSocket,
    deadline: Option<tokio::time::Instant>,
) -> Option<Option<Result<Message, axum::Error>>> {
    match deadline {
        Some(d) => tokio::time::timeout_at(d, socket.recv()).await.ok(),
        None => Some(socket.recv().await),
    }
}

/// Output of a running task or an explanation.
enum Out {
    Message(ServerMessage),
    /// The task (or build, or program) ended.
    Finished,
    /// The explanation ended.
    Explained,
}

/// The error sent instead of a task when the chosen offer is not there.
fn rejected(e: OfferRejected) -> ServerMessage {
    ServerMessage::Error {
        error: e.to_string(),
        code: Some(e.code().into()),
    }
}

/// Runs a task on a blocking thread; its messages go to `tx`. The browser needs access to the
/// project (native dialog the first time); only then `prepare` makes the task (it may look at the
/// project) or tells why there is none. The first task of the browser in the project starts after
/// the person answers Start in the native dialog.
fn start(
    state: &AppState,
    origin: String,
    key_id: String,
    project: String,
    prepare: impl FnOnce(&dyn Backend) -> Result<TaskRequest, OfferRejected> + Send + 'static,
    tx: mpsc::UnboundedSender<Out>,
) -> Cancel {
    let cancel = Cancel::new();
    let (backend, auth, c) = (state.backend.clone(), state.auth.clone(), cancel.clone());
    let started = state.started.clone();
    tokio::task::spawn_blocking(move || {
        if auth.project_allowed(&key_id, &origin, &project) {
            match prepare(backend.as_ref()) {
                Ok(task)
                    if !started.started(&key_id, &project)
                        && !auth.start_work_allowed(&origin, &project, &task.prompt) =>
                {
                    let _ = tx.send(Out::Message(ServerMessage::Error {
                        error: "start not confirmed".into(),
                        code: Some("start_not_confirmed".into()),
                    }));
                }
                Ok(task) => {
                    started.start(&key_id, &project);
                    backend.run_task(
                        task,
                        &mut |m| {
                            let _ = tx.send(Out::Message(m));
                        },
                        &c,
                    );
                }
                Err(e) => {
                    let _ = tx.send(Out::Message(rejected(e)));
                }
            }
        } else {
            let _ = tx.send(Out::Message(ServerMessage::Error {
                error: "no access to this project".into(),
                code: Some("project_forbidden".into()),
            }));
        }
        let _ = tx.send(Out::Finished);
    });
    cancel
}

async fn send(socket: &mut WebSocket, message: &ServerMessage) -> Result<(), axum::Error> {
    let json = serde_json::to_string(message).unwrap_or_default();
    socket.send(Message::Text(json.into())).await
}

async fn close(mut socket: WebSocket, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: CLOSE_AUTH_FAILED,
            reason: reason.into(),
        })))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(m: Option<ServerMessage>) -> Option<String> {
        match m {
            Some(ServerMessage::Error { code, .. }) => code,
            _ => None,
        }
    }

    #[test]
    fn a_missing_offer_is_a_coded_error() {
        for (e, expected) in [
            (OfferRejected::NoOffers, "no_offers"),
            (OfferRejected::BadIndex, "bad_index"),
            (OfferRejected::NotSupported, "not_supported"),
        ] {
            assert_eq!(code(Some(rejected(e))).as_deref(), Some(expected));
        }
    }

    #[test]
    fn a_note_goes_only_to_a_running_task_of_its_project() {
        let task = Running {
            cancel: Cancel::new(),
            task_of: Some("notes-app".into()),
        };
        assert_eq!(
            note(Some(&task), "notes-app", "blue".into(), Vec::new()),
            None
        );
        assert_eq!(task.cancel.take_notes().len(), 1);
        assert_eq!(
            code(note(Some(&task), "other", "blue".into(), Vec::new())).as_deref(),
            Some("no_task")
        );
        assert_eq!(
            code(note(None, "notes-app", "blue".into(), Vec::new())).as_deref(),
            Some("no_task")
        );
        let build = Running {
            cancel: Cancel::new(),
            task_of: None,
        };
        assert_eq!(
            code(note(Some(&build), "notes-app", "blue".into(), Vec::new())).as_deref(),
            Some("no_task")
        );
        assert_eq!(
            code(note(Some(&task), "notes-app", String::new(), Vec::new())).as_deref(),
            Some("note_rejected")
        );
        task.cancel.cancel();
        assert_eq!(
            code(note(Some(&task), "notes-app", "late".into(), Vec::new())).as_deref(),
            Some("no_task")
        );
    }
}
