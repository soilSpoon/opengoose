---
# CI Data Analysis
# Shared module for analyzing CI run data
#
# Usage:
#   imports:
#     - shared/ci-data-analysis.md
#
# This import provides:
# - Pre-download CI runs and artifacts
# - Build and test the project
# - Collect performance metrics

imports:
  - shared/jqschema.md

tools:
  cache-memory: true
  bash: ["*"]

steps:
  - name: Download CI workflow runs from last 7 days
    env:
      GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    run: |
      # Download workflow runs for the ci workflow
      gh run list --repo ${{ github.repository }} --workflow=ci-quality-gate.yml --limit 100 --json databaseId,status,conclusion,createdAt,updatedAt,displayTitle,headBranch,event,url,workflowDatabaseId,number > /tmp/ci-runs.json
      
      # Create directory for artifacts
      mkdir -p /tmp/ci-artifacts
      
      # Download artifacts from recent runs (last 5 successful runs)
      echo "Downloading artifacts from recent CI runs..."
      gh run list --repo ${{ github.repository }} --workflow=ci-quality-gate.yml --status success --limit 5 --json databaseId | jq -r '.[].databaseId' | while read -r run_id; do
        echo "Processing run $run_id"
        gh run download "$run_id" --repo ${{ github.repository }} --dir "/tmp/ci-artifacts/$run_id" 2>/dev/null || echo "No artifacts for run $run_id"
      done
      
      echo "CI runs data saved to /tmp/ci-runs.json"
      echo "Artifacts saved to /tmp/ci-artifacts/"
      
      # Summarize downloaded artifacts
      echo "## Downloaded Artifacts" >> "$GITHUB_STEP_SUMMARY"
      find /tmp/ci-artifacts -type f -name "*.txt" -o -name "*.html" -o -name "*.json" | head -20 | while read -r f; do
        echo "- $(basename "$f")" >> "$GITHUB_STEP_SUMMARY"
      done
  
  - name: Setup Rust toolchain
    uses: dtolnay/rust-toolchain@stable
    with:
      components: clippy, rustfmt
  
  - name: Install system dependencies
    run: sudo apt-get update && sudo apt-get install -y libxcb1-dev libdbus-1-dev pkg-config
  
  - name: Cache cargo registry and build
    uses: actions/cache@v4
    with:
      path: |
        ~/.cargo/registry
        ~/.cargo/git
        target
      key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
      restore-keys: |
        ${{ runner.os }}-cargo-
  
  - name: Check formatting
    run: cargo fmt --all --check
  
  - name: Run clippy
    run: cargo clippy --workspace --all-targets --all-features
  
  - name: Build workspace
    run: cargo build --workspace
  
  - name: Run unit tests
    continue-on-error: true
    run: |
      mkdir -p /tmp/ci-test-results
      cargo test --workspace -- --format json 2>/tmp/ci-test-results/test-stderr.log | tee /tmp/ci-test-results/test-results.json
---

# CI Data Analysis

Pre-downloaded CI run data and artifacts are available for analysis:

## Available Data

1. **CI Runs**: `/tmp/ci-runs.json`
   - Last 100 workflow runs with status, timing, and metadata
   
2. **Artifacts**: `/tmp/ci-artifacts/`
   - Coverage reports and benchmark results from recent successful runs
   
3. **CI Configuration**: `.github/workflows/ci-quality-gate.yml`
   - Current CI workflow configuration
   
4. **Cache Memory**: `/tmp/cache-memory/`
   - Historical analysis data from previous runs
   
5. **Test Results**: `/tmp/ci-test-results/test-results.json`
   - JSON output from Rust unit tests with performance and timing data

## Test Case Locations

Rust test cases are located throughout the workspace crates:
- **Inline unit tests**: `crates/*/src/*.rs` (inside `#[cfg(test)]` modules)
- **Integration tests**: `crates/*/tests/*.rs`

## Environment Setup

The workflow has already completed:
- ✅ **Formatting**: Code formatted with `cargo fmt --all --check`
- ✅ **Linting**: Clippy run with `cargo clippy --workspace --all-targets --all-features`
- ✅ **Building**: Workspace built with `cargo build --workspace`
- ✅ **Testing**: Unit tests run (with performance data collected in JSON format)

This means you can:
- Make changes to code or configuration files
- Validate changes immediately by running `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features`, or `cargo test --workspace`
- Ensure proposed optimizations don't break functionality before creating a PR

## Analyzing Run Data

Parse the downloaded CI runs data:

```bash
# Analyze run data
cat /tmp/ci-runs.json | jq '
{
  total_runs: length,
  by_status: group_by(.status) | map({status: .[0].status, count: length}),
  by_conclusion: group_by(.conclusion) | map({conclusion: .[0].conclusion, count: length}),
  by_branch: group_by(.headBranch) | map({branch: .[0].headBranch, count: length}),
  by_event: group_by(.event) | map({event: .[0].event, count: length})
}'
```

**Metrics to extract:**
- Success rate per job
- Average duration per job
- Failure patterns (which jobs fail most often)
- Cache hit rates from step summaries
- Resource usage patterns

## Review Artifacts

Examine downloaded artifacts for insights:

```bash
# List downloaded artifacts
find /tmp/ci-artifacts -type f -name "*.txt" -o -name "*.html" -o -name "*.json"

# Analyze coverage reports if available
# Check benchmark results for performance trends
```

## Historical Context

Check cache memory for previous analyses:

```bash
# Read previous optimization recommendations
if [ -f /tmp/cache-memory/ci-coach/last-analysis.json ]; then
  cat /tmp/cache-memory/ci-coach/last-analysis.json
fi

# Check if previous recommendations were implemented
# Compare current metrics with historical baselines
```
