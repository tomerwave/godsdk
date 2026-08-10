use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use godsdk_core::{GenerationMode, GenerationRequest, ReferencePolicy, Target, generate};

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
    Validate(ValidateArgs),
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

    #[arg(
        long = "remote-ref-host",
        value_name = "HOST",
        help = "Allow remote $ref documents from this host; repeat for multiple hosts"
    )]
    remote_ref_hosts: Vec<String>,

    #[arg(
        long = "remote-ref-pin",
        value_name = "URL=SHA256",
        help = "Pin a remote $ref document by URL and SHA-256; repeat for multiple documents"
    )]
    remote_ref_pins: Vec<String>,
}

#[derive(Debug, Args)]
struct ValidateArgs {
    #[arg(short = 's', long, alias = "source")]
    spec: PathBuf,

    #[arg(long = "remote-ref-host", value_name = "HOST")]
    remote_ref_hosts: Vec<String>,

    #[arg(long = "remote-ref-pin", value_name = "URL=SHA256")]
    remote_ref_pins: Vec<String>,
}

pub fn run(cli: Cli) -> Result<(), godsdk_core::GenerationError> {
    match cli.command {
        Command::Generate(args) => run_generate(args),
        Command::Validate(args) => run_validate(args),
    }
}

fn run_generate(args: GenerateArgs) -> Result<(), godsdk_core::GenerationError> {
    let mut request = GenerationRequest::new(args.source, args.output);
    request.mode = generation_mode(args.dry_run, args.check);
    request = request.with_targets(parse_targets(&args.targets)?);
    request.prune = args.prune;
    request.reference_policy = reference_policy(&args.remote_ref_hosts, &args.remote_ref_pins)?;
    let result = generate(&request)?;
    report_dry_run(request.mode, result.files);
    Ok(())
}

fn report_dry_run(mode: GenerationMode, files: Vec<std::path::PathBuf>) {
    let prefix = if mode == GenerationMode::DryRun {
        "would change"
    } else {
        "changed"
    };
    for path in files {
        println!("{prefix} {}", path.display());
    }
}

fn run_validate(args: ValidateArgs) -> Result<(), godsdk_core::GenerationError> {
    let policy = reference_policy(&args.remote_ref_hosts, &args.remote_ref_pins)?;
    let spec = godsdk_core::ApiSpec::from_path_with_policy(args.spec, &policy)?;
    println!(
        "valid OpenAPI {}: {} ({} operations, {} schemas)",
        spec.openapi_version,
        spec.title,
        spec.operations.len(),
        spec.schemas.len()
    );
    Ok(())
}

fn reference_policy(
    hosts: &[String],
    pins: &[String],
) -> Result<ReferencePolicy, godsdk_core::GenerationError> {
    let policy = hosts.iter().cloned().fold(
        ReferencePolicy::default(),
        ReferencePolicy::allow_remote_host,
    );
    pins.iter().map(String::as_str).try_fold(policy, add_pin)
}

fn add_pin(
    policy: ReferencePolicy,
    pin: &str,
) -> Result<ReferencePolicy, godsdk_core::GenerationError> {
    let (url, checksum) = pin.split_once('=').ok_or_else(|| {
        godsdk_core::GenerationError::InvalidReferencePolicy("pins must use URL=SHA256".to_string())
    })?;
    validate_checksum(url, checksum).map(|checksum| policy.pin_remote_reference(url, checksum))
}

fn validate_checksum(url: &str, checksum: &str) -> Result<String, godsdk_core::GenerationError> {
    if url.is_empty() || checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(godsdk_core::GenerationError::InvalidReferencePolicy(
            format!("invalid SHA-256 pin for {url}"),
        ));
    }
    Ok(checksum.to_ascii_lowercase())
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
