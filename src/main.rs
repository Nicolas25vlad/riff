use std::{env, process};

use riff::{Command, run};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    match Command::parse(&args).and_then(run) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
        }
        Err(err) => {
            eprintln!("riff: {err}");
            process::exit(1);
        }
    }
}
