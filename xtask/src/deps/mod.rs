// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

mod config;
mod crates_io;
mod pr_generator;
mod updater;
mod version;

pub use config::Config;
pub use crates_io::CratesIoClient;
pub use pr_generator::PrGenerator;
pub use updater::DependencyUpdater;
pub use version::VersionChecker;

use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CheckDepsOptions {
    pub config_path: PathBuf,
    pub dry_run: bool,
    pub no_push: bool,
    pub ci: bool,
}

pub async fn check_dependencies(options: CheckDepsOptions) -> Result<()> {
    // Load configuration
    let config = Config::load(&options.config_path)?;

    // Initialize components
    let crates_io = CratesIoClient::new()?;
    let version_checker = VersionChecker::new();
    let updater = DependencyUpdater::new()?;
    let pr_generator = PrGenerator::new();

    println!("🔍 Checking dependencies for major version updates...");

    let mut updates_found = false;

    for (dep_name, dep_config) in &config.dependencies {
        if !dep_config.enabled {
            continue;
        }

        println!("\n📦 Checking {}...", dep_name);

        // Get latest version from crates.io
        let latest_version = match crates_io.get_latest_major_version(dep_name).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  ⚠️  Failed to fetch version: {}", e);
                continue;
            }
        };

        // Get current version from Cargo.toml
        let current_version = match updater.get_current_version(dep_name) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  ⚠️  Failed to get current version: {}", e);
                continue;
            }
        };

        println!("  Current: {}", current_version);
        println!("  Latest:  {}", latest_version);

        // Check if update is needed
        if !version_checker.is_major_update(&current_version, &latest_version) {
            println!("  ✓ Up to date");
            continue;
        }

        println!("  🎉 Major update available!");
        updates_found = true;

        if options.dry_run {
            println!(
                "  [DRY RUN] Would update {} to {}",
                dep_name, latest_version
            );
            continue;
        }

        // Perform the update
        match updater.update_dependency(
            dep_name,
            &latest_version.to_string(),
            &dep_config.bump_strategy,
        ) {
            Ok(_) => println!("  ✓ Updated successfully"),
            Err(e) => {
                eprintln!("  ✗ Failed to update: {}", e);
                continue;
            }
        }

        // Generate PR description
        let pr_description = pr_generator.generate(
            dep_name,
            &current_version.to_string(),
            &latest_version.to_string(),
        );

        if !options.no_push && options.ci {
            // In CI mode, create the PR
            println!("  📝 Creating pull request...");
            match pr_generator.create_pr(
                dep_name,
                &latest_version.to_string(),
                &pr_description,
                &config.settings.labels,
            ) {
                Ok(_) => println!("  ✓ Pull request created"),
                Err(e) => eprintln!("  ✗ Failed to create PR: {}", e),
            }
        }
    }

    if !updates_found {
        println!("\n✨ All dependencies are up to date!");
    }

    Ok(())
}
