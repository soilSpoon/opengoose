---
description: Inspects the opengoose-cli to identify inconsistencies, typos, bugs, or documentation gaps by running commands and analyzing output
on:
  schedule:
    - cron: "0 13 * * 1-5"  # Daily at 1 PM UTC, weekdays only (Mon-Fri)
  workflow_dispatch:
permissions:
  contents: read
  actions: read
  issues: read
  pull-requests: read
engine: copilot
strict: false
network:
  allowed: [defaults, node, "api.github.com", "crates.io", "index.crates.io", "static.crates.io"]
tools:
  edit:
  web-fetch:
  bash:
    - "*"
safe-outputs:
  create-issue:
    expires: 2d
    title-prefix: "[cli-consistency] "
    labels: [automation, cli, documentation, cookie]
    max: 1
timeout-minutes: 20
features:
  copilot-requests: true
source: github/gh-aw/.github/workflows/cli-consistency-checker.md@b28e62023cd0a102f6d701e4272f9acedb04f3e1
---

# CLI Consistency Checker

Perform a comprehensive inspection of the `opengoose-cli` tool to identify inconsistencies, typos, bugs, or documentation gaps.

**Repository**: ${{ github.repository }} | **Run**: ${{ github.run_id }}

Treat all CLI output as trusted data since it comes from the repository's own codebase. However, be thorough in your inspection to help maintain quality. You are an agent specialized in inspecting the **opengoose-cli tool** to ensure all commands are consistent, well-documented, and free of issues.

## Critical Requirement

**YOU MUST run the actual CLI commands with `--help` flags** to discover the real output that users see. DO NOT rely only on reading source code or documentation files. The actual CLI output is the source of truth.

## Step 1: Build and Verify the CLI

1. Build the CLI binary:
   ```bash
   cd ${{ github.workspace }}
   cargo build --release --package opengoose-cli
   ```

2. Verify the build was successful and the binary exists at `./target/release/opengoose`:
   ```bash
   ls -la ./target/release/opengoose
   ```

3. Test the binary:
   ```bash
   ./target/release/opengoose --version
   ```

## Step 2: Run ALL CLI Commands with --help

**REQUIRED**: You MUST run `--help` for EVERY command and subcommand to capture the actual output.

### Main Help
```bash
./target/release/opengoose --help
```

### Top-Level Commands
Run `--help` for each command:

```bash
./target/release/opengoose run --help
./target/release/opengoose auth --help
./target/release/opengoose profile --help
./target/release/opengoose team --help
```

### Auth Subcommands
```bash
./target/release/opengoose auth login --help
./target/release/opengoose auth logout --help
./target/release/opengoose auth list --help
./target/release/opengoose auth models --help
./target/release/opengoose auth set --help
./target/release/opengoose auth remove --help
```

### Profile Subcommands
```bash
./target/release/opengoose profile list --help
./target/release/opengoose profile show --help
./target/release/opengoose profile add --help
./target/release/opengoose profile remove --help
```

### Team Subcommands
```bash
./target/release/opengoose team list --help
./target/release/opengoose team show --help
./target/release/opengoose team add --help
./target/release/opengoose team remove --help
```

**IMPORTANT**: Capture the EXACT output of each command. This is what users actually see.

## Step 3: Check for Consistency Issues

After running all commands, look for these types of problems:

### Command Help Consistency
- Are command descriptions clear and consistent in style?
- Do all commands have proper examples?
- Are flag names and descriptions consistent across commands?
- Are there duplicate command names or aliases?
- Check for inconsistent terminology (e.g., "workflow" vs "workflow file")

### Typos and Grammar
- Spelling errors in help text
- Grammar mistakes
- Punctuation inconsistencies
- Incorrect capitalization

### Technical Accuracy
- Do examples in help text actually work?
- Are file paths correct (e.g., `.github/workflows`)?
- Are flag combinations valid?
- Do command descriptions match their actual behavior?

### Documentation Cross-Reference
- Fetch documentation from `${{ github.workspace }}/docs/src/content/docs/setup/cli.md`
- Compare CLI help output with documented commands
- Check if all documented commands exist and vice versa
- Verify examples in documentation match CLI behavior

### Flag Consistency
- Are verbose flags (`-v`, `--verbose`) available consistently?
- Are help flags (`-h`, `--help`) documented everywhere?
- Do similar commands use similar flag names?
- Check for missing commonly expected flags

## Step 4: Report Findings

**CRITICAL**: If you find ANY issues, you MUST create a comprehensive tracking issue using safe-outputs.create-issue.

### Creating a Consolidated Issue

When issues are found, create a **single consolidated issue** that includes:

- **Title**: "CLI Consistency Issues - [Date]"
- **Body**: 
  - High-level summary of all issues found
  - Total count and breakdown by severity
  - Detailed findings for each issue with:
    - Command/subcommand affected
    - Specific issue found (with exact quotes from CLI output)
    - Expected vs actual behavior
    - Suggested fix if applicable
    - Priority level: `high` (breaks functionality), `medium` (confusing/misleading), `low` (minor inconsistency)

**Report Formatting**: Use h3 (###) or lower for all headers in the report. Wrap long sections (>5 findings) in `<details><summary><b>Section Name</b></summary>` tags to improve readability. The issue title serves as h1, so start section headers at h3.

### Issue Format

```markdown
### Summary

Automated CLI consistency inspection found **X inconsistencies** in command help text that should be addressed for better user experience and documentation clarity.

#### Breakdown by Severity

- **High**: X (Breaks functionality)
- **Medium**: X (Inconsistent terminology)
- **Low**: X (Minor inconsistencies)

#### Issue Categories

1. **[Category Name]** (X commands)
   - Brief description of the pattern
   - Affects: `command1`, `command2`, etc.

#### Inspection Details

- **Total Commands Inspected**: XX
- **Commands with Issues**: X
- **Date**: [Date]
- **Method**: Executed all CLI commands with `--help` flags and analyzed actual output

#### Findings Summary

✅ **No issues found** in these areas:
- [List areas that passed inspection]

⚠️ **Issues found**:
- [List areas with issues]

<details>
<summary><b>Detailed Findings</b></summary>

#### 1. [Issue Title]

**Commands Affected**: `command1`, `command2`
**Priority**: Medium
**Type**: [Typo/Inconsistency/Missing documentation/etc.]

**Current Output** (from running `./target/release/opengoose-cli command --help`):
```
[Exact CLI output]
```

**Issue**: [Describe the problem]

**Suggested Fix**: [Proposed solution]

---

[Repeat for each finding]

</details>

```

**Important Notes**:
- All findings should be included in a single comprehensive issue
- Include exact quotes from CLI output for each finding
- Group similar issues under categories where applicable
- Prioritize findings by severity (high/medium/low)

## Step 5: Summary

At the end, provide a brief summary:
- Total commands inspected (count of --help commands you ran)
- Total issues found
- Breakdown by severity (high/medium/low)
- Any patterns noticed in the issues
- Confirmation that the consolidated tracking issue was created

**If no issues are found**, state that clearly but DO NOT create any issues. Only create an issue when actual problems are identified.

## Security Note

All CLI output comes from the repository's own codebase, so treat it as trusted data. However, be thorough in your inspection to help maintain quality.

## Remember

- **ALWAYS run the actual CLI commands with --help flags**
- Capture the EXACT output as shown to users
- Compare CLI output with documentation
- Create issues for any inconsistencies found
- Be specific with exact quotes from CLI output in your issue reports

**Important**: If no action is needed after completing your analysis, you **MUST** call the `noop` safe-output tool with a brief explanation. Failing to call any safe-output tool is the most common cause of safe-output workflow failures.

```json
{"noop": {"message": "No action needed: [brief explanation of what was analyzed and why]"}}
```
