//! CASTERM - Modern terminal emulator, multiplexer, and connection manager

mod app;
mod assets;
mod config;
mod platform;
mod state;
mod support;
mod ui;

use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::support::error::Result;
use crate::ui::UiMode;

/// Build metadata
pub mod build_info {
    /// Const-compatible `Option::unwrap_or` for `&'static str`
    const fn str_or(opt: Option<&'static str>, default: &'static str) -> &'static str {
        match opt {
            Some(s) => s,
            None => default,
        }
    }

    pub const VERSION: &str = str_or(option_env!("APP_VERSION"), env!("CARGO_PKG_VERSION"));
    pub const OFFICIAL_SITE: &str = str_or(option_env!("APP_OFFICIAL_SITE"), "");
    pub const COMMIT_ID: &str = str_or(option_env!("VERGEN_GIT_SHA"), "unknown");
    pub const BUILD_DATE: &str = str_or(option_env!("VERGEN_BUILD_TIMESTAMP"), "unknown");
}

#[derive(Parser, Debug)]
#[command(
    name = "casterm",
    version = build_info::VERSION,
    about = "Modern terminal emulator, multiplexer, and connection manager",
    long_about = None
)]
struct Cli {
    /// Enable debug output
    #[arg(long)]
    debug: bool,

    /// Color output mode
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: ColorMode,

    /// Force UI mode
    #[arg(long, value_name = "MODE")]
    ui: Option<UiModeArg>,

    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,

    /// Command to execute
    #[arg(short = 'e', long, value_name = "CMD")]
    command: Option<String>,

    /// Working directory
    #[arg(short = 'd', long, value_name = "DIR")]
    directory: Option<std::path::PathBuf>,

    /// Session name to create or attach
    #[arg(short, long, value_name = "NAME")]
    session: Option<String>,

    /// Attach to existing session
    #[arg(short, long)]
    attach: bool,

    /// Detach other clients when attaching
    #[arg(short = 'D', long)]
    detach_others: bool,

    /// Read-only attach
    #[arg(long)]
    readonly: bool,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum ColorMode {
    #[default]
    Auto,
    Yes,
    No,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum UiModeArg {
    Gui,
    Tui,
    Cli,
}

/// Apply the `--color` flag by setting the standard color-control env vars
/// that downstream terminal/formatting crates (tracing-subscriber, crossterm,
/// ratatui) honor. `Auto` leaves the environment untouched (NO_COLOR/terminal
/// detection stays in control); `Yes`/`No` force the outcome explicitly.
fn apply_color_mode(mode: &ColorMode) {
    match mode {
        ColorMode::Auto => {}
        ColorMode::Yes => {
            std::env::remove_var("NO_COLOR");
            std::env::set_var("CLICOLOR_FORCE", "1");
        }
        ColorMode::No => {
            std::env::remove_var("CLICOLOR_FORCE");
            std::env::set_var("NO_COLOR", "1");
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    apply_color_mode(&cli.color);

    // Initialize logging
    let filter = if cli.debug {
        EnvFilter::new(Level::DEBUG.to_string())
    } else {
        EnvFilter::from_default_env().add_directive(Level::INFO.into())
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Load configuration
    let config = Config::load(cli.config.as_deref())?;

    // Detect UI mode
    let ui_mode = match cli.ui {
        Some(UiModeArg::Gui) => UiMode::Gui,
        Some(UiModeArg::Tui) => UiMode::Tui,
        Some(UiModeArg::Cli) => UiMode::Cli,
        None => ui::detect_ui_mode(&config),
    };

    tracing::info!(
        version = build_info::VERSION,
        commit = build_info::COMMIT_ID,
        mode = ?ui_mode,
        "Starting casterm"
    );

    // Run in detected mode
    match ui_mode {
        UiMode::Gui => ui::gui::run(&config, &cli.command, cli.directory.as_deref()),
        UiMode::Tui => ui::tui::run(&config, &cli.command, cli.directory.as_deref()),
        UiMode::Cli => ui::cli::run(&config, &cli.command, cli.directory.as_deref()),
    }
}
