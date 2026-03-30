// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub settings: Settings,
    pub dependencies: HashMap<String, DependencyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub schedule: String,
    pub branch_prefix: String,
    pub labels: Vec<String>,
    pub draft_prs: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyConfig {
    pub enabled: bool,
    pub track: String,         // "major", "minor", "patch"
    pub bump_strategy: String, // "major", "minor", "patch"
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config =
            serde_yaml::from_str(&content).with_context(|| "Failed to parse config file")?;

        // Validate config version
        if config.version != 1 {
            anyhow::bail!("Unsupported config version: {}", config.version);
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parsing() {
        let yaml = r#"
version: 1
settings:
  schedule: "0 10 * * 1"
  branch_prefix: "auto-deps"
  labels: ["dependencies", "major-update"]
  draft_prs: false
dependencies:
  ractor:
    enabled: true
    track: major
    bump_strategy: minor
"#;

        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.settings.branch_prefix, "auto-deps");
        assert_eq!(config.dependencies.len(), 1);
        assert!(config.dependencies.get("ractor").unwrap().enabled);
    }
}
