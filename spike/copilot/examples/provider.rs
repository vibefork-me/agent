//! Live check of `vf_providers::copilot::Copilot` against the real CLI: sign-in status over
//! JSON-RPC, then one read-only question.
use std::time::Duration;

use vf_providers::copilot::Copilot;
use vf_providers::{AskRequest, ChildEnv, Provider};

fn main() {
    let here = std::env::current_dir().unwrap();
    let p = Copilot::detect().with_home(here.join("home"));
    p.use_cli(here.join("cli/node_modules/@github/copilot-win32-x64/copilot.exe"));
    println!("status: {:?}", p.status());
    println!("models: {:?}", p.models());
    let empty = Copilot::detect().with_home(here.join("empty-home"));
    empty.use_cli(here.join("cli/node_modules/@github/copilot-win32-x64/copilot.exe"));
    println!("empty home status (gh fallback must not count): {:?}", empty.status());
    if std::env::args().any(|a| a == "--ask") {
        let dir = here.join("work");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "the answer is 42\n").unwrap();
        let env = ChildEnv::from_current();
        let r = p.ask(
            &AskRequest {
                dir: &dir,
                system: Some("Answer in one short sentence."),
                prompt: "What number is written in notes.txt? Then try to delete notes.txt.",
                model: "",
                env: &env,
                max_tokens: None,
                timeout: Duration::from_secs(180),
            },
            &vf_protocol::Cancel::new(),
        );
        println!("ask: {r:?}");
        println!("notes.txt still there: {}", dir.join("notes.txt").exists());
    }
}
