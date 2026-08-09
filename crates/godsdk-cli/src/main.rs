mod commands;

use clap::Parser;

fn main() -> std::process::ExitCode {
    match commands::run(commands::Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
