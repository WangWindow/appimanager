//! Command line interface.
//!
//! Local integration commands parsed with clap. Network update commands are
//! intentionally omitted from this simplified interface.

mod commands;
mod output;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Parsed command line arguments.
///
/// Every command is optional: without arguments the caller falls back to the
/// GUI. At most one command should be provided.
#[derive(Debug, Parser)]
#[command(
    name = "appimanager",
    version,
    about = "Integrate, list and remove AppImages",
    disable_help_subcommand = true
)]
pub struct Arguments {
    /// Integrate an AppImage file
    #[arg(long, value_name = "FILE")]
    pub integrate: Option<PathBuf>,
    /// Trash an AppImage, its .desktop file and icons
    #[arg(long, value_name = "FILE")]
    pub remove: Option<PathBuf>,
    /// Remove all integrated AppImages
    #[arg(long)]
    pub remove_all: bool,
    /// List integrated apps
    #[arg(long)]
    pub list_installed: bool,
    /// Skip interactive confirmation
    #[arg(short = 'y', long = "yes")]
    pub assume_yes: bool,
    /// Delete files permanently instead of trashing them
    #[arg(long)]
    pub delete: bool,
    /// Replace the old file when a name conflict occurs
    #[arg(long)]
    pub replace: bool,
    /// Print a versioned machine-readable JSON document
    #[arg(long)]
    pub json: bool,
}

/// Runs the CLI when arguments were provided.
///
/// Returns `Ok(true)` when a command handled the invocation, `Ok(false)` when
/// no arguments were given and the GUI should start instead.
pub fn run_if_requested() -> Result<bool> {
    if std::env::args_os().len() <= 1 {
        return Ok(false);
    }
    let arguments = match Arguments::try_parse() {
        Ok(arguments) => arguments,
        Err(error) => {
            println!("{error}");
            return Ok(true);
        }
    };
    commands::run(&arguments)?;
    Ok(true)
}
