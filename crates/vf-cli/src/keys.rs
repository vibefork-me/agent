//! `vibefork keys` — API keys in the OS secret store: `keys` lists which are set, `keys set <name>`
//! reads the key from stdin, `keys remove <name>` deletes it.

use std::io::BufRead as _;
use std::process::ExitCode;

use vf_platform::secrets;

pub(crate) fn run(args: &[String]) -> ExitCode {
    match args {
        [] => {
            for name in secrets::NAMES {
                let state = if secrets::get(name).is_some() {
                    "set"
                } else {
                    "not set"
                };
                println!("{name:<12} {state}");
            }
            ExitCode::SUCCESS
        }
        [cmd, name] if cmd == "set" => {
            eprintln!("paste the {name} key and press Enter:");
            let mut line = String::new();
            if std::io::stdin().lock().read_line(&mut line).is_err() || line.trim().is_empty() {
                eprintln!("vibefork: no key given");
                return ExitCode::FAILURE;
            }
            report(secrets::set(name, line.trim()), "stored")
        }
        [cmd, name] if cmd == "remove" => report(secrets::remove(name), "removed"),
        _ => {
            eprintln!(
                "usage: vibefork keys [set <name> | remove <name>]   names: {}",
                secrets::NAMES.join(", ")
            );
            ExitCode::from(2)
        }
    }
}

fn report(result: Result<(), secrets::SecretError>, done: &str) -> ExitCode {
    match result {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("vibefork: {e}");
            ExitCode::FAILURE
        }
    }
}
