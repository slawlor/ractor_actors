// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use anyhow::{Context, Result};
use semver::Version;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use toml_edit::{value, DocumentMut};

use super::VersionChecker;

pub struct DependencyUpdater {
    workspace_root: PathBuf,
    ractor_actors_manifest: PathBuf,
    version_checker: VersionChecker,
}

impl DependencyUpdater {
    pub fn new() -> Result<Self> {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("Failed to get workspace root")?
            .to_path_buf();

        let ractor_actors_manifest = workspace_root.join("ractor_actors/Cargo.toml");

        Ok(Self {
            workspace_root,
            ractor_actors_manifest,
            version_checker: VersionChecker::new(),
        })
    }

    /// Get the current version of a dependency from Cargo.toml
    pub fn get_current_version(&self, dep_name: &str) -> Result<Version> {
        let content = fs::read_to_string(&self.ractor_actors_manifest)
            .context("Failed to read Cargo.toml")?;

        let doc = content
            .parse::<DocumentMut>()
            .context("Failed to parse Cargo.toml")?;

        let version_str = doc["dependencies"][dep_name]["version"]
            .as_str()
            .or_else(|| doc["dependencies"][dep_name].as_str())
            .context(format!("Dependency {} not found", dep_name))?;

        // Parse version requirement (e.g., "0.15.8" or "^0.15")
        let version_req = version_str.trim_start_matches('^').trim_start_matches('~');

        // Try to parse as a complete version first
        if let Ok(v) = Version::parse(version_req) {
            return Ok(v);
        }

        // If not a complete version, append .0 as needed
        let parts: Vec<&str> = version_req.split('.').collect();
        let complete_version = match parts.len() {
            1 => format!("{}.0.0", parts[0]),
            2 => format!("{}.{}.0", parts[0], parts[1]),
            _ => version_req.to_string(),
        };

        Version::parse(&complete_version)
            .context(format!("Failed to parse version: {}", version_str))
    }

    /// Update a dependency to a new version and bump ractor_actors version
    pub fn update_dependency(
        &self,
        dep_name: &str,
        new_version: &str,
        bump_strategy: &str,
    ) -> Result<()> {
        let content = fs::read_to_string(&self.ractor_actors_manifest)
            .context("Failed to read Cargo.toml")?;

        let mut doc = content
            .parse::<DocumentMut>()
            .context("Failed to parse Cargo.toml")?;

        // Update dependency version
        if let Some(dep_table) = doc["dependencies"][dep_name].as_table_like_mut() {
            dep_table.insert("version", value(new_version));
        } else if doc["dependencies"][dep_name].is_str() {
            doc["dependencies"][dep_name] = value(new_version);
        } else {
            anyhow::bail!("Dependency {} not found or has unexpected format", dep_name);
        }

        // Bump ractor_actors version
        let current_version_str = doc["package"]["version"]
            .as_str()
            .context("Failed to get current ractor_actors version")?;

        let current_version =
            Version::parse(current_version_str).context("Failed to parse current version")?;

        let new_ractor_version = self
            .version_checker
            .bump_version(&current_version, bump_strategy);
        doc["package"]["version"] = value(new_ractor_version.to_string());

        // Write updated Cargo.toml
        fs::write(&self.ractor_actors_manifest, doc.to_string())
            .context("Failed to write Cargo.toml")?;

        // Run cargo update to update Cargo.lock
        self.update_lockfile(dep_name)?;

        println!(
            "  ✓ Updated {} to {} (ractor_actors {} -> {})",
            dep_name, new_version, current_version, new_ractor_version
        );

        Ok(())
    }

    /// Update Cargo.lock for the specific dependency
    fn update_lockfile(&self, dep_name: &str) -> Result<()> {
        let output = Command::new("cargo")
            .args(["update", "-p", dep_name])
            .current_dir(&self.workspace_root)
            .output()
            .context("Failed to run cargo update")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("cargo update failed: {}", stderr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_version() {
        // This test requires the actual Cargo.toml file
        let updater = DependencyUpdater::new().unwrap();
        let version = updater.get_current_version("ractor");
        assert!(version.is_ok());
    }
}
