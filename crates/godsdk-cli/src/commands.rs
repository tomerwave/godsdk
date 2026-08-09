use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use godsdk_core::{GenerationRequest, generate};

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
}

pub fn run(cli: Cli) -> Result<(), godsdk_core::GenerationError> {
    let Command::Generate(args) = cli.command;
    let request = GenerationRequest::new(args.source, args.output);
    generate(&request).map(|_| ())
}
