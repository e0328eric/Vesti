#![allow(clippy::enum_variant_names)]
#![deny(bindings_with_variant_name)]

mod commands;
mod error;
mod exit_status;
mod lexer;
mod location;
mod parser;

use std::fs::File;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use clap::Parser;

use signal_hook::consts::signal::{SIGINT, SIGKILL, SIGTERM};
use signal_hook::flag as signal_flag;

use crate::commands::{compile_vesti, VestiOpt};
use crate::error::pretty_print::pretty_print;
use crate::exit_status::ExitCode;

fn main() -> ExitCode {
    let args = commands::VestiOpt::parse();

    if let VestiOpt::Init = args {
        File::create("source.ves").expect("ERROR: cannot create a file");
    } else {
        let is_continuous = args.is_continuous_compile();

        let trap = Arc::new(AtomicUsize::new(0));
        #[cfg(not(target_os = "windows"))]
        for signal in [SIGINT, SIGTERM].iter() {
            signal_flag::register_usize(*signal, Arc::clone(&trap), *signal as usize)
                .expect("Undefined behavior happened!");
        }
        // TODO: I do not test this code in windows actually :)
        #[cfg(target_os = "windows")]
        for signal in [SIGINT, SIGTERM, SIGKILL].iter() {
            signal_flag::register_usize(*signal, Arc::clone(&trap), *signal as usize)
                .expect("Undefined behavior happened!");
        }

        let file_lists = match args.take_file_name() {
            Ok(inner) => inner,
            Err(err) => {
                println!("{}", pretty_print(None, err, None));
                std::process::exit(1);
            }
        };

        let mut handle_vesti: Vec<JoinHandle<ExitCode>> = Vec::new();
        for file_name in file_lists {
            handle_vesti.push(thread::spawn(move || {
                compile_vesti(file_name, is_continuous)
            }));
        }

        if !is_continuous {
            let has_issue = handle_vesti
                .into_iter()
                .map(|vesti| vesti.join().unwrap())
                .any(|exit_code| exit_code != ExitCode::Success);
            if has_issue {
                return ExitCode::Failure;
            }
        } else {
            println!("Press Ctrl+C to finish the program.");
            while ![SIGINT, SIGTERM, SIGKILL].contains(&(trap.load(Ordering::Relaxed) as i32)) {
                thread::sleep(Duration::from_millis(500));
            }
        }

        println!("bye!");
    }

    ExitCode::Success
}
