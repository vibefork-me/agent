//! `vibefork` without arguments in a terminal: status and a short menu.

use std::io::{BufRead as _, Write as _};
use std::process::ExitCode;

pub(crate) fn run() -> ExitCode {
    loop {
        let _ = crate::agent::status(false);
        println!();
        println!("  1  start / stop the agent     4  check this machine (doctor)");
        println!("  2  open the interface         5  storage folders");
        println!("  3  paired browsers            6  update");
        println!("  q  quit");
        print!("> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
            return ExitCode::SUCCESS;
        }
        println!();
        match line.trim() {
            "1" => {
                let s = vf_platform::Settings::from_env();
                if crate::agent::hello(s.port).is_some() {
                    crate::agent::stop();
                } else {
                    crate::agent::start();
                }
            }
            "2" => {
                crate::agent::open(false);
            }
            "3" => {
                crate::setup::pairs(&[]);
            }
            "4" => {
                crate::doctor::run(false);
            }
            "5" => {
                crate::storage::run(&[]);
            }
            "6" => {
                crate::update::run(&[]);
            }
            "q" | "Q" | "" => return ExitCode::SUCCESS,
            other => println!("unknown choice `{other}`"),
        }
        println!();
    }
}
