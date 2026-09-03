use std::{env, process};

use riff::{Command, run};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

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
