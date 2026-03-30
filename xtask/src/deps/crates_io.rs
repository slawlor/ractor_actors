// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use anyhow::{Context, Result};
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

const CRATES_IO_API: &str = "https://crates.io/api/v1";
const USER_AGENT: &str = "ractor-dependency-tracker (https://github.com/slawlor/ractor)";

#[derive(Debug, Deserialize)]
struct CrateResponse {
    versions: Vec<VersionInfo>,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    num: String,
    yanked: bool,
}

pub struct CratesIoClient {
    client: reqwest::Client,
}

impl CratesIoClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    pub async fn get_latest_major_version(&self, crate_name: &str) -> Result<Version> {
        let url = format!("{}/crates/{}", CRATES_IO_API, crate_name);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to crates.io")?;

        if !response.status().is_success() {
            anyhow::bail!("crates.io API returned error: {}", response.status());
        }

        let crate_info: CrateResponse = response
            .json()
            .await
            .context("Failed to parse crates.io response")?;

        // Find the latest non-yanked, stable version
        let latest_version = crate_info
            .versions
            .iter()
            .filter(|v| !v.yanked)
            .filter_map(|v| Version::parse(&v.num).ok())
            .filter(|v| v.pre.is_empty()) // Filter out pre-releases
            .max()
            .context("No stable versions found")?;

        Ok(latest_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_latest_version() {
        let client = CratesIoClient::new().unwrap();
        let version = client.get_latest_major_version("serde").await.unwrap();
        assert!(version.major >= 1);
    }
}
