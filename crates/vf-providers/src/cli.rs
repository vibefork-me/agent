//! Running a coding CLI that prints JSON lines.

use serde_json::Value;
use vf_protocol::{Cancel, Event};

use crate::process::{self, Stream};
use crate::{Output, RunRequest, RunResult, Tracker, clip};

/// Runs a CLI to the end and collects its stdout (for one-shot questions).
pub(crate) fn collect(
    bin: Option<&std::path::Path>,
    name: &str,
    args: &[String],
    req: &crate::AskRequest<'_>,
    cancel: &Cancel,
) -> Result<String, String> {
    let bin = bin.ok_or_else(|| format!("{name} CLI not found"))?;
    let mut cmd = req.env.command(bin);
    cmd.args(args).current_dir(req.dir);
    let mut stdout = String::new();
    let mut stderr = String::new();
    let exit = process::run_streaming(
        cmd,
        Some(req.prompt),
        cancel,
        Some(req.timeout),
        &mut |stream, line| {
            let buf = if stream == Stream::Stdout {
                &mut stdout
            } else {
                &mut stderr
            };
            buf.push_str(&line);
            buf.push('\n');
        },
    )
    .map_err(|e| format!("cannot start {name}: {e}"))?;
    if exit.cancelled {
        return Err("cancelled".into());
    }
    if exit.timed_out {
        return Err(format!("{name}: no answer in {}s", req.timeout.as_secs()));
    }
    if exit.code != 0 && stdout.trim().is_empty() {
        return Err(format!(
            "{name} exited with {}: {}",
            exit.code,
            clip(stderr.trim(), 300)
        ));
    }
    Ok(stdout)
}

/// Runs a JSON-lines CLI: prompt to stdin, every stdout line normalized, stderr passed on.
pub(crate) fn run_cli(
    bin: Option<&std::path::Path>,
    name: &str,
    args: Vec<String>,
    req: &RunRequest<'_>,
    out: &mut dyn FnMut(Output),
    cancel: &Cancel,
    normalize: fn(&Value) -> Vec<Event>,
) -> RunResult {
    let mut tracker = Tracker::new(req.resume);
    let Some(bin) = bin else {
        out(Output::Event(Event::Error {
            text: format!("{name} CLI not found"),
        }));
        return tracker.finish(-1, false);
    };
    let mut cmd = req.env.command(bin);
    cmd.args(&args).current_dir(req.dir);
    let exit =
        process::run_streaming(
            cmd,
            Some(req.prompt),
            cancel,
            None,
            &mut |stream, line| match stream {
                Stream::Stdout => {
                    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
                        return;
                    };
                    for ev in normalize(&v) {
                        tracker.observe(&ev);
                        out(Output::Event(ev));
                    }
                }
                Stream::Stderr => {
                    let t = line.trim();
                    if !t.is_empty()
                        && !t.contains("Reading additional input from stdin")
                        && !t.contains("Reading prompt from stdin")
                    {
                        out(Output::Stderr(clip(t, 2000)));
                    }
                }
            },
        );
    match exit {
        Ok(e) => tracker.finish(e.code, e.cancelled),
        Err(e) => {
            out(Output::Event(Event::Error {
                text: format!("cannot start {name}: {e}"),
            }));
            tracker.finish(-1, false)
        }
    }
}
