//! Running a project's program and sending its output to the editor, bounded by
//! [`vf_core::limits::PROGRAM_LINE`] and [`vf_core::limits::PROGRAM_OUTPUT`].

use std::time::Instant;

use vf_builds::detect_run;
use vf_core::limits;
use vf_protocol::Cancel;
use vf_protocol::ws::{ProgramStream, ServerMessage};
use vf_providers::process::{self, Stream};

use crate::Engine;
use crate::build::shell;

/// What of a program's output is still sent: lines cut to a length, all of it up to a total.
#[derive(Debug)]
pub(crate) struct OutputBudget {
    line: usize,
    left: usize,
    cut: bool,
}

impl OutputBudget {
    /// Lines of at most `line` bytes, at most `total` bytes in all.
    pub(crate) fn new(line: usize, total: usize) -> Self {
        Self {
            line,
            left: total,
            cut: false,
        }
    }

    /// The part of `text` to send, or `None` once the total is spent. A line longer than the
    /// bound keeps its first characters that fit.
    pub(crate) fn admit(&mut self, text: &str) -> Option<String> {
        if self.left == 0 {
            self.cut = true;
            return None;
        }
        let room = self.line.min(self.left);
        let mut kept = String::new();
        for c in text.chars() {
            if kept.len().saturating_add(c.len_utf8()) > room {
                self.cut = true;
                break;
            }
            kept.push(c);
        }
        // An empty line still costs one byte, so endless empty output ends too.
        self.left = self.left.saturating_sub(kept.len().max(1));
        Some(kept)
    }

    /// Whether some output was cut or dropped.
    pub(crate) fn cut(&self) -> bool {
        self.cut
    }
}

impl Engine {
    /// Runs the program of `project` (see [`vf_builds::detect_run`]) in the same environment as its
    /// build, until it exits or `cancel`.
    pub(crate) fn run_project_program(
        &self,
        project: &str,
        out: &mut dyn FnMut(ServerMessage),
        cancel: &Cancel,
    ) {
        let error = |out: &mut dyn FnMut(ServerMessage), e: String, code: &str| {
            out(ServerMessage::Error {
                error: e,
                code: Some(code.into()),
            })
        };
        let Ok(dir) = self.projects.resolve(project) else {
            return error(out, "unknown project".into(), "not_found");
        };
        let Some(step) = detect_run(&dir) else {
            return error(
                out,
                "no known way to run this project".into(),
                "no_run_recipe",
            );
        };
        let run_id = crate::step::new_run_id(&format!("run-{project}"));
        let _tmp_guard = crate::step::TmpGuard::new(&self.caches, &run_id);
        let (env, _, _) = self.child_env(&dir, true, false, Some(&run_id), out, cancel);
        let mut cmd = shell(&dir, &step.program, &step.args, &env);
        cmd.current_dir(&dir);
        let mut budget = OutputBudget::new(limits::PROGRAM_LINE, limits::PROGRAM_OUTPUT);
        let started = Instant::now();
        let exit = process::run_streaming(cmd, None, cancel, None, &mut |stream, line| {
            if let Some(text) = budget.admit(&line) {
                let stream = match stream {
                    Stream::Stdout => ProgramStream::Stdout,
                    Stream::Stderr => ProgramStream::Stderr,
                };
                out(ServerMessage::ProgramLog { stream, text });
            }
        });
        match exit {
            Ok(e) if e.cancelled => out(ServerMessage::Cancelled),
            Ok(e) => out(ServerMessage::ProgramDone {
                code: e.code,
                cut: budget.cut(),
                seconds: started.elapsed().as_secs_f64(),
            }),
            Err(e) => error(out, format!("cannot start the program: {e}"), "run_failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OutputBudget;

    #[test]
    fn long_lines_are_cut_on_a_character_boundary() {
        let mut b = OutputBudget::new(5, 100);
        assert_eq!(b.admit("abc").as_deref(), Some("abc"));
        assert!(!b.cut());
        // "é" is two bytes: "abcé" is 5, the next "é" does not fit.
        assert_eq!(b.admit("abcéé").as_deref(), Some("abcé"));
        assert!(b.cut());
    }

    #[test]
    fn output_past_the_total_is_dropped() {
        let mut b = OutputBudget::new(10, 6);
        assert_eq!(b.admit("1234").as_deref(), Some("1234"));
        assert_eq!(b.admit("5678").as_deref(), Some("56"));
        assert_eq!(b.admit("9"), None);
        assert!(b.cut());
        // Empty lines are not free.
        let mut e = OutputBudget::new(10, 2);
        assert!(e.admit("").is_some() && e.admit("").is_some());
        assert_eq!(e.admit(""), None);
    }
}
