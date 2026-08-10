use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use godsdk_core::{GenerationMode, GenerationRequest, generate};

#[derive(Debug, Parser)]
#[command(
    name = "godsdk",
    version,
    about = "Generate strongly typed SDKs from API specifications."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Generate(GenerateArgs),
}

#[derive(Debug, Args)]
struct GenerateArgs {
    #[arg(short = 's', long)]
    source: PathBuf,

    #[arg(short = 'o', long)]
    output: PathBuf,

    #[arg(long, conflicts_with = "check")]
    dry_run: bool,

    #[arg(long, conflicts_with = "dry_run")]
    check: bool,
}

pub fn run(cli: Cli) -> Result<(), godsdk_core::GenerationError> {
    let Command::Generate(args) = cli.command;
    let mut request = GenerationRequest::new(args.source, args.output);
    request.mode = generation_mode(args.dry_run, args.check);
    let result = generate(&request)?;
    if request.mode == GenerationMode::DryRun {
        for path in result.files {
            println!("would change {}", path.display());
        }
    }
    Ok(())
}

fn generation_mode(dry_run: bool, check: bool) -> GenerationMode {
    match (dry_run, check) {
        (true, false) => GenerationMode::DryRun,
        (false, true) => GenerationMode::Check,
        (false, false) | (true, true) => GenerationMode::Write,
    }
}
