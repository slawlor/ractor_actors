// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

mod deps;

use anyhow::Result;
use clap::{Parser, Subcommand};
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
        None => {
            // Fall back to xtaskops for other commands
            xtaskops::tasks::main()
        }
    }
}
