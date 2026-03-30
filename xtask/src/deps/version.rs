// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use semver::Version;

pub struct VersionChecker;

impl VersionChecker {
    pub fn new() -> Self {
        Self
    }

    /// Check if the new version is a major update from the current version
    pub fn is_major_update(&self, current: &Version, latest: &Version) -> bool {
        latest.major > current.major
    }

    /// Check if the new version is a minor update from the current version
    #[allow(dead_code)]
    pub fn is_minor_update(&self, current: &Version, latest: &Version) -> bool {
        latest.major == current.major && latest.minor > current.minor
    }

    /// Check if the new version is a patch update from the current version
    #[allow(dead_code)]
    pub fn is_patch_update(&self, current: &Version, latest: &Version) -> bool {
        latest.major == current.major
            && latest.minor == current.minor
            && latest.patch > current.patch
    }

    /// Bump version according to strategy
    pub fn bump_version(&self, current: &Version, strategy: &str) -> Version {
        match strategy {
            "major" => Version::new(current.major + 1, 0, 0),
            "minor" => Version::new(current.major, current.minor + 1, 0),
            "patch" => Version::new(current.major, current.minor, current.patch + 1),
            _ => current.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_major_update() {
        let checker = VersionChecker::new();
        let current = Version::parse("0.15.8").unwrap();
        let latest = Version::parse("0.16.0").unwrap();
        assert!(checker.is_major_update(&current, &latest));

        let current = Version::parse("0.15.8").unwrap();
        let latest = Version::parse("0.15.9").unwrap();
        assert!(!checker.is_major_update(&current, &latest));
    }

    #[test]
    fn test_bump_version() {
        let checker = VersionChecker::new();
        let current = Version::parse("0.5.0").unwrap();

        let major_bump = checker.bump_version(&current, "major");
        assert_eq!(major_bump, Version::parse("1.0.0").unwrap());

        let minor_bump = checker.bump_version(&current, "minor");
        assert_eq!(minor_bump, Version::parse("0.6.0").unwrap());

        let patch_bump = checker.bump_version(&current, "patch");
        assert_eq!(patch_bump, Version::parse("0.5.1").unwrap());
    }
}
