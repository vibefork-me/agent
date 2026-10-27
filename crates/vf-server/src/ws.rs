//! `/v1/ws`: Origin is checked by the guard on upgrade; the first message must authenticate the
//! connection (5 s). A plain `ping` before authentication is answered with `echo: ping`
//! (connectivity check without a session).
//!
//! After authentication: `task` starts a task (one at a time per connection), `cancel` cancels it,
//! `stop` pauses it, `resume` continues it, cancelling a paused task undoes its changes; `note`
//! queues a message for the running task of that project (checked against the bounds of
//! `vf_protocol::note`; otherwise an error); `choose_offer`, `rollback_part` answer `not_supported`
//! for now, other text is echoed. Messages are at most [`MAX_MESSAGE`] bytes. A running task also sends its session stream (`canvas` messages). Closing the connection cancels the running task.

use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, State};
use axum::response::Response;
use tokio::sync::mpsc;
use vf_protocol::note::NoteRejected;
use vf_protocol::ws::{CLOSE_AUTH_FAILED, ClientMessage, ServerMessage};
use vf_protocol::{Cancel, TaskRequest};

use crate::AppState;
use crate::guard::EffectiveOrigin;

/// Time a connection has to authenticate.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest WebSocket message, bytes: a `note` with its attachments fits.
const MAX_MESSAGE: usize = vf_protocol::note::MAX_NOTE_DATA + 1024 * 1024;

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
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
    let mut running: Option<Running> = None;
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
                    let cancel = start(&state, origin.clone(), key, task, tx.clone());
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
                    let cancel = start_build(&state, origin.clone(), key, project, tx.clone());
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
            Ok(ClientMessage::Note {
                project,
                text,
                attachments,
            }) => note(running.as_ref(), &project, text, attachments),
            // The protocol has them; the engine handles them in later versions.
            Ok(ClientMessage::ChooseOffer { .. } | ClientMessage::RollbackPart { .. }) => {
                Some(ServerMessage::Error {
                    error: "not supported by this version of the agent".into(),
                    code: Some("not_supported".into()),
                })
            }
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
    // The browser left: a running task is cancelled.
    if let Some(r) = running {
        r.cancel.cancel();
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

/// Builds after the user confirms it in the native dialog (invariant 3).
fn start_build(
    state: &AppState,
    origin: String,
    key_id: String,
    project: String,
    tx: mpsc::UnboundedSender<Out>,
) -> Cancel {
    let cancel = Cancel::new();
    let (backend, auth, c) = (state.backend.clone(), state.auth.clone(), cancel.clone());
    tokio::task::spawn_blocking(move || {
        let lang = auth.dialog().language();
        let prompt = vf_core::catalog::action_build(lang);
        if auth.project_allowed(&key_id, &origin, &project)
            && auth.confirm_action(&origin, prompt, Some(&project))
        {
            backend.build(
                &project,
                &mut |m| {
                    let _ = tx.send(Out::Message(m));
                },
                &c,
            );
        } else {
            let _ = tx.send(Out::Message(ServerMessage::Error {
                error: "build not confirmed".into(),
                code: Some("build_not_confirmed".into()),
            }));
        }
        let _ = tx.send(Out::Finished);
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

/// Output of a running task.
enum Out {
    Message(ServerMessage),
    Finished,
}

/// Runs a task on a blocking thread; its messages go to `tx`. The browser needs access to the
/// project (native dialog the first time).
fn start(
    state: &AppState,
    origin: String,
    key_id: String,
    task: TaskRequest,
    tx: mpsc::UnboundedSender<Out>,
) -> Cancel {
    let cancel = Cancel::new();
    let (backend, auth, c) = (state.backend.clone(), state.auth.clone(), cancel.clone());
    tokio::task::spawn_blocking(move || {
        if auth.project_allowed(&key_id, &origin, &task.project) {
            backend.run_task(
                task,
                &mut |m| {
                    let _ = tx.send(Out::Message(m));
                },
                &c,
            );
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
