# Dependency Tracker Implementation

## Overview

This document describes the automated dependency tracking system for `ractor_actors`.

## Purpose

The dependency tracker automatically monitors major version updates for critical dependencies and creates pull requests with appropriate version bumps when updates are detected.

### Why Major Versions Only?

- **Dependabot handles minor/patch**: Daily Dependabot runs already cover minor and patch updates
- **Breaking changes matter**: Major versions often include breaking changes that need careful review
- **Reduce noise**: Focus on significant updates rather than every patch release
- **Manual review required**: Major updates need migration planning and testing

## Tracked Dependencies

| Dependency | Current | Reason |
|------------|---------|--------|
| `ractor` | 0.15.8 | Core framework - critical to all functionality |
| `chrono` | 0.4.x | Time feature - date/time handling |
| `cron` | 0.16.x | Time feature - scheduled tasks |
| `notify` | 8.x | Filewatcher feature - file system monitoring |

## How It Works

### 1. Weekly Schedule

- Runs every Monday at 10:00 AM UTC
- Can be triggered manually via GitHub Actions UI

### 2. Version Check Process

```
For each tracked dependency:
  1. Query crates.io API for latest version
  2. Parse current version from Cargo.toml
  3. Compare major versions
  4. If new major version exists → proceed to update
```

### 3. Update Process

When a major version update is detected:

1. **Create branch**: `auto-deps/{dependency}-{version}`
2. **Update Cargo.toml**: Set new dependency version
3. **Bump ractor_actors version**: Minor bump (0.5.0 → 0.6.0)
4. **Update Cargo.lock**: Run `cargo update -p {dependency}`
5. **Commit changes**: With descriptive message
6. **Create PR**: With changelog links and migration notes

### 4. Version Bump Strategy

All major dependency updates trigger a **minor version bump** of `ractor_actors`:

| Dependency Update | ractor_actors Version | Reasoning |
|-------------------|----------------------|-----------|
| ractor 0.15 → 0.16 | 0.5.0 → 0.6.0 | Core framework change - API may change |
| chrono 0.4 → 0.5 | 0.5.0 → 0.6.0 | Time API changes affect time feature |
| cron 0.16 → 0.17 | 0.5.0 → 0.6.0 | Cron API changes affect time feature |
| notify 8 → 9 | 0.5.0 → 0.6.0 | File watching API changes affect filewatcher |

This follows semantic versioning: minor bumps for backward-compatible additions, which major dependency updates may introduce.

## Manual Usage

### Dry Run (Recommended First)

```bash
cargo xtask check-deps --dry-run
```

Shows what updates are available without making any changes.

### Local Testing

```bash
cargo xtask check-deps --no-push
```

Creates branch and commits locally but doesn't push or create PR.

### Full CI Mode

```bash
cargo xtask check-deps --ci
```

Creates branches, pushes, and creates PRs. This is what the GitHub Action runs.

## Configuration

Edit `.github/dependency-tracker.yml` to modify settings:

```yaml
version: 1

settings:
  schedule: "0 10 * * 1"  # Cron schedule
  branch_prefix: "auto-deps"  # Branch naming
  labels: ["dependencies", "major-update", "automated"]  # PR labels
  draft_prs: false  # Create as draft PRs

dependencies:
  ractor:
    enabled: true  # Enable/disable tracking
    track: major  # Which updates to track (major/minor/patch)
    bump_strategy: minor  # How to bump ractor_actors version
```

## Implementation Details

### File Structure

```
xtask/
  src/
    deps/
      mod.rs           - Main orchestration logic
      config.rs        - Configuration file parser
      crates_io.rs     - crates.io API client
      version.rs       - Version comparison and bumping
      updater.rs       - Cargo.toml file updater
      pr_generator.rs  - PR creation and description

.github/
  workflows/
    dependency-tracker.yml  - GitHub Actions workflow
  dependency-tracker.yml    - Configuration file
```

### Key Components

#### 1. CratesIoClient

Queries crates.io API with proper rate limiting and User-Agent:

```rust
pub async fn get_latest_major_version(&self, crate_name: &str) -> Result<Version>
```

#### 2. VersionChecker

Compares versions using semver crate:

```rust
pub fn is_major_update(&self, current: &Version, latest: &Version) -> bool
pub fn bump_version(&self, current: &Version, strategy: &str) -> Version
```

#### 3. DependencyUpdater

Updates Cargo.toml using toml_edit (preserves formatting):

```rust
pub fn update_dependency(&self, dep_name: &str, new_version: &str, bump_strategy: &str) -> Result<()>
```

#### 4. PrGenerator

Creates PRs using GitHub CLI:

```rust
pub fn create_pr(&self, dep_name: &str, new_version: &str, description: &str, labels: &[String]) -> Result<()>
```

## Error Handling

### API Failures
- Retries with exponential backoff
- Respects crates.io rate limits
- Logs errors but continues with other dependencies

### Version Parsing Errors
- Skips dependency if version can't be parsed
- Logs warning with details
- Continues with other dependencies

### Git/PR Creation Errors
- Checks if branch already exists (skips if so)
- Checks if PR already exists (updates if possible)
- Fails gracefully with actionable error messages

## Integration with Existing Systems

### Dependabot

- **Dependabot**: Daily minor/patch updates
- **Dependency Tracker**: Weekly major updates
- **No conflicts**: Different branch prefixes (`dependabot/` vs `auto-deps/`)

### CI Pipeline

All automated PRs trigger the existing CI workflow:

- `cargo test` - Run all tests
- `cargo clippy` - Lint checks
- `cargo fmt` - Format checks
- `cargo doc` - Documentation builds

### Manual Review

- **No auto-merge**: All PRs require manual approval
- **Breaking changes**: Reviewer must check changelog
- **Migration notes**: Included in PR description
- **Test locally**: Reviewer can pull branch and test

## Troubleshooting

### Workflow doesn't run

1. Check GitHub Actions is enabled for the repository
2. Verify the workflow file is on the main branch
3. Check the schedule cron syntax
4. Manually trigger with `workflow_dispatch`

### API rate limiting

If hitting crates.io rate limits:

1. Reduce number of tracked dependencies
2. Increase schedule interval (e.g., bi-weekly)
3. Add delays between API calls

### PR creation fails

Common causes:

1. Missing GitHub token permissions (needs `contents: write` and `pull-requests: write`)
2. Branch already exists from previous run
3. GitHub CLI (`gh`) not configured properly
4. Base branch doesn't exist

### Version parsing issues

If a dependency version can't be parsed:

1. Check if it uses a non-standard version format
2. Look for pre-release tags that should be filtered
3. Verify the dependency exists on crates.io

## Future Enhancements

Potential improvements:

1. **Smart scheduling**: Run more frequently for critical deps
2. **Auto-merge**: For patch updates that pass CI
3. **Changelog parsing**: Automatically extract breaking changes
4. **Slack notifications**: Alert on new PRs
5. **Rollback detection**: Detect when updates cause CI failures
6. **Batch updates**: Group related dependencies in single PR

## Testing

Before deploying changes:

```bash
# 1. Test dry run
cargo xtask check-deps --dry-run

# 2. Test local updates (no push)
cargo xtask check-deps --no-push

# 3. Verify git state
git status
git diff

# 4. Test workflow syntax
actionlint .github/workflows/dependency-tracker.yml  # if installed

# 5. Test in CI (create a test branch)
git checkout -b test-dependency-tracker
git push origin test-dependency-tracker
# Trigger workflow_dispatch on test branch
```

## Security Considerations

1. **No secrets in config**: Configuration is public in `.github/`
2. **GITHUB_TOKEN**: Only has permissions for this repository
3. **No external services**: Only uses crates.io and GitHub APIs
4. **Code review required**: All PRs need manual approval before merge
5. **Audit trail**: All changes tracked in git history and PR comments
