// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

mod deps;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::ffi::OsStr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Task automation for ractor_actors", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check for dependency updates and optionally create PRs
    CheckDeps {
        /// Path to the dependency tracker config file
        #[arg(long, default_value = ".github/dependency-tracker.yml")]
        config: PathBuf,

        /// Dry run mode - show what would be updated without making changes
        #[arg(long)]
        dry_run: bool,

        /// Don't push changes or create PRs (local only)
        #[arg(long)]
        no_push: bool,

        /// CI mode - create PRs automatically
        #[arg(long)]
        ci: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    if !is_dependency_tracker_command(std::env::args_os().nth(1).as_deref()) {
        return xtaskops::tasks::main();
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::CheckDeps {
            config,
            dry_run,
            no_push,
            ci,
        }) => {
            let options = deps::CheckDepsOptions {
                config_path: config,
                dry_run,
                no_push,
                ci,
            };
            deps::check_dependencies(options).await
        }
        None => unreachable!("the check-deps command was detected before parsing"),
    }
}

fn is_dependency_tracker_command(command: Option<&OsStr>) -> bool {
    command == Some(OsStr::new("check-deps"))
}

#[cfg(test)]
mod tests {
    use super::is_dependency_tracker_command;
    use std::ffi::OsStr;

    #[test]
    fn only_check_deps_uses_the_custom_cli() {
        assert!(is_dependency_tracker_command(Some(OsStr::new(
            "check-deps"
        ))));
        assert!(!is_dependency_tracker_command(Some(OsStr::new("coverage"))));
        assert!(!is_dependency_tracker_command(None));
    }
}
