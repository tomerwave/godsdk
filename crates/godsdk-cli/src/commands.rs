use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use godsdk_core::{GenerationMode, GenerationRequest, Target, generate};

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

    #[arg(long, default_value = "rust,typescript")]
    targets: String,

    #[arg(
        long,
        help = "Delete stale generated files after checking for user edits"
    )]
    prune: bool,
}

pub fn run(cli: Cli) -> Result<(), godsdk_core::GenerationError> {
    let Command::Generate(args) = cli.command;
    let mut request = GenerationRequest::new(args.source, args.output);
    request.mode = generation_mode(args.dry_run, args.check);
    request = request.with_targets(parse_targets(&args.targets)?);
    request.prune = args.prune;
    let result = generate(&request)?;
    if request.mode == GenerationMode::DryRun {
        for path in result.files {
            println!("would change {}", path.display());
        }
    }
    Ok(())
}

fn parse_targets(value: &str) -> Result<Vec<Target>, godsdk_core::GenerationError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(|target| match target {
            "rust" => Ok(Target::Rust),
            "python" => Ok(Target::Python),
            "typescript" => Ok(Target::TypeScript),
            other => Err(godsdk_core::GenerationError::InvalidTarget(
                other.to_string(),
            )),
        })
        .collect()
}

fn generation_mode(dry_run: bool, check: bool) -> GenerationMode {
    match (dry_run, check) {
        (true, false) => GenerationMode::DryRun,
        (false, true) => GenerationMode::Check,
        (false, false) | (true, true) => GenerationMode::Write,
    }
}
