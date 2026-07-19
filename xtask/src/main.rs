//! CASTERM build automation tasks

use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "CASTERM build tasks")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Build release binary
    Build {
        /// Target triple
        #[arg(long)]
        target: Option<String>,
    },
    /// Run tests
    Test,
    /// Run clippy
    Lint,
    /// Format code
    Fmt,
    /// Generate shell completions
    Completions {
        /// Output directory
        #[arg(long, default_value = "completions")]
        out_dir: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Task::Build { target } => build(target.as_deref()),
        Task::Test => test(),
        Task::Lint => lint(),
        Task::Fmt => fmt(),
        Task::Completions { out_dir } => completions(&out_dir),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn build(target: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--release"]);

    if let Some(t) = target {
        cmd.args(["--target", t]);
    }

    run_command(cmd)
}

fn test() -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--all-features"]);
    run_command(cmd)
}

fn lint() -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args(["clippy", "--all-features", "--", "-D", "warnings"]);
    run_command(cmd)
}

fn fmt() -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--all"]);
    run_command(cmd)
}

fn completions(out_dir: &str) -> Result<(), String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    eprintln!("Generating completions to {}/", out_dir);
    // TODO: Generate completions using clap_complete
    Ok(())
}

fn run_command(mut cmd: Command) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("Failed to run command: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Command failed with exit code: {:?}", status.code()))
    }
}
