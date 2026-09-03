use std::{env, fs, process};

use riff::{Command, Playlist, run};

mod tui;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().is_some_and(|arg| arg == "tui") {
        let Some(path) = args.get(1) else {
            eprintln!("riff: tui requires a .riff playlist\n\nUSAGE:\n  riff tui <file.riff>");
            process::exit(1);
        };
        if args.len() != 2 {
            eprintln!("riff: invalid tui arguments\n\nUSAGE:\n  riff tui <file.riff>");
            process::exit(1);
        }

        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("riff: {err}");
                process::exit(1);
            }
        };
        let playlist = match Playlist::parse(&source) {
            Ok(playlist) => playlist,
            Err(err) => {
                eprintln!("riff: {err}");
                process::exit(1);
            }
        };

        if let Err(err) = tui::run(playlist).await {
            eprintln!("riff: tui error: {err}");
            process::exit(1);
        }
        return;
    }

    match Command::parse(&args) {
        Ok(command) => match run(command).await {
            Ok(output) => {
                if !output.is_empty() {
                    println!("{output}");
                }
            }
            Err(err) => {
                eprintln!("riff: {err}");
                process::exit(1);
            }
        },
        Err(err) => {
            eprintln!("riff: {err}");
            process::exit(1);
        }
    }
}
