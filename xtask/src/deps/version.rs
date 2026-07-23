// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

use semver::{Version, VersionReq};

pub struct VersionChecker;

impl Default for VersionChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionChecker {
    pub fn new() -> Self {
        Self
    }

    /// Check if `latest` is a breaking (Cargo-major) update from `current`.
    ///
    /// Uses Cargo semver-compatibility semantics: for `0.x.y` crates, a bump
    /// in the leftmost non-zero component is breaking (e.g. `0.15.8` → `0.16.0`
    /// is treated the same as `1.15.8` → `2.0.0`).
    pub fn is_major_update(&self, current: &Version, latest: &Version) -> bool {
        let req = VersionReq::parse(&format!("^{current}"))
            .expect("^<version> is always a valid VersionReq");
        latest > current && !req.matches(latest)
    }

    /// Check if `latest` is a compatible bump within the same Cargo-major.
    #[allow(dead_code)]
    pub fn is_minor_update(&self, current: &Version, latest: &Version) -> bool {
        let req = VersionReq::parse(&format!("^{current}"))
            .expect("^<version> is always a valid VersionReq");
        latest > current && req.matches(latest)
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

        // Cargo-major bump on a 0.x crate: 0.15.8 → 0.16.0 is breaking.
        let current = Version::parse("0.15.8").unwrap();
        let latest = Version::parse("0.16.0").unwrap();
        assert!(checker.is_major_update(&current, &latest));

        // Patch bump within same Cargo-major: 0.15.8 → 0.15.9 is not breaking.
        let current = Version::parse("0.15.8").unwrap();
        let latest = Version::parse("0.15.9").unwrap();
        assert!(!checker.is_major_update(&current, &latest));

        // Post-1.0: 1.2.3 → 2.0.0 is breaking.
        let current = Version::parse("1.2.3").unwrap();
        let latest = Version::parse("2.0.0").unwrap();
        assert!(checker.is_major_update(&current, &latest));

        // Post-1.0: 1.2.3 → 1.3.0 is not breaking.
        let current = Version::parse("1.2.3").unwrap();
        let latest = Version::parse("1.3.0").unwrap();
        assert!(!checker.is_major_update(&current, &latest));

        // Same version is not an update.
        let current = Version::parse("0.15.8").unwrap();
        assert!(!checker.is_major_update(&current, &current));
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
