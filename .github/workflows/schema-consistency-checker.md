---
on:
  schedule: daily
  workflow_dispatch: null
permissions:
  contents: read
  issues: read
  pull-requests: read
imports:
- github/gh-aw/.github/workflows/shared/reporting.md@b28e62023cd0a102f6d701e4272f9acedb04f3e1
safe-outputs:
  create-issue:
    close-older-issues: true
    expires: 1d
    max: 1
    title-prefix: "[Schema Consistency] "
    labels: [automation, schema, documentation]
description: Detects inconsistencies between JSON schema, implementation code, and documentation
engine: copilot
source: github/gh-aw/.github/workflows/schema-consistency-checker.md@b28e62023cd0a102f6d701e4272f9acedb04f3e1
timeout-minutes: 30
tools:
  bash:
  - "*"
  cache-memory:
    key: schema-consistency-cache-${{ github.workflow }}
  edit: null
  github:
    mode: remote
    toolsets:
    - default
---
# Schema Consistency Checker

You are an expert system that detects inconsistencies between:
- The main JSON schema of the frontmatter (`pkg/parser/schemas/main_workflow_schema.json`)
- The Rust crate implementations (`crates/*/src/*.rs`)
- The documentation (`docs/src/content/docs/**/*.md`)
- The workflows in the project (`.github/workflows/*.md`)

## Mission

Analyze the repository to find inconsistencies across these four key areas and create an issue report with actionable findings.

## Cache Memory Strategy Storage

Use the cache memory folder at `/tmp/gh-aw/cache-memory/` to store and reuse successful analysis strategies:

1. **Read Previous Strategies**: Check `/tmp/gh-aw/cache-memory/strategies.json` for previously successful detection methods
2. **Strategy Selection**: 
   - 70% of the time: Use a proven strategy from the cache
   - 30% of the time: Try a radically different approach to discover new inconsistencies
   - Implementation: Use the day of year (e.g., `date +%j`) modulo 10 to determine selection: values 0-6 use proven strategies, 7-9 try new approaches
3. **Update Strategy Database**: After analysis, save successful strategies to `/tmp/gh-aw/cache-memory/strategies.json`

Strategy database structure:
```json
{
  "strategies": [
    {
      "id": "strategy-1",
      "name": "Schema field enumeration check",
      "description": "Compare schema enum values with parser constants",
      "success_count": 5,
      "last_used": "2024-01-15",
      "findings": 3
    }
  ],
  "last_updated": "2024-01-15"
}
```

## Analysis Areas

### 1. Schema vs Implementation

**Check for:**
- Fields defined in schema but not handled in Rust crate code
- Fields handled in Rust code but missing from schema
- Type mismatches (schema says `string`, Rust code expects a struct)
- Enum values in schema not validated in Rust code
- Required fields not enforced
- Default values inconsistent between schema and Rust code

**Key files to analyze:**
- `pkg/parser/schemas/main_workflow_schema.json`
- `pkg/parser/schemas/mcp_config_schema.json`
- `crates/*/src/*.rs` - All Rust source files across crates
- Look for serde attributes (`#[serde(rename = "...")]`, `#[serde(default)]`)
- Look for struct definitions that map to schema fields
- Look for deserialization and validation logic

### 2. Schema vs Documentation

**Check for:**
- Schema fields not documented
- Documented fields not in schema
- Type descriptions mismatch
- Example values that violate schema
- Missing or outdated examples
- Enum values documented but not in schema

**Key files to analyze:**
- `docs/src/content/docs/reference/frontmatter.md`
- `docs/src/content/docs/reference/frontmatter-full.md`
- `docs/src/content/docs/reference/*.md` (all reference docs)

### 3. Schema vs Actual Workflows

**Check for:**
- Workflows using fields not in schema
- Workflows using deprecated fields
- Invalid field values according to schema
- Missing required fields
- Type violations in actual usage
- Undocumented field combinations

**Key files to analyze:**
- `.github/workflows/*.md` (all workflow files)
- `.github/workflows/shared/**/*.md` (shared components)

### 4. Implementation vs Documentation

**Check for:**
- Rust crate features not documented
- Documented features not implemented in Rust code
- Error messages that don't match docs
- Validation rules not documented

**Focus on:**
- `crates/*/src/*.rs` - All Rust implementation files

## Detection Strategies

Here are proven strategies you can use or build upon:

### Strategy 1: Field Enumeration Diff
1. Extract all field names from schema
2. Extract all field names from Rust code (look for serde attributes, struct field names)
3. Extract all field names from documentation
4. Compare and find missing/extra fields

### Strategy 2: Type Analysis
1. For each field in schema, note its type
2. Search Rust code for how that field is processed
3. Check if types match
4. Report type mismatches

### Strategy 3: Enum Validation
1. Extract enum values from schema
2. Search for those enums in Rust validation code
3. Check if all enum values are handled
4. Find undocumented enum values

### Strategy 4: Example Validation
1. Extract code examples from documentation
2. Validate each example against the schema
3. Report examples that don't validate
4. Suggest corrections

### Strategy 5: Real-World Usage Analysis
1. Parse all workflow files in the repo
2. Extract frontmatter configurations
3. Check each against schema
4. Find patterns that work but aren't in schema (potential missing features)

### Strategy 6: Grep-Based Pattern Detection
1. Use bash/grep to find specific patterns
2. Example: `grep -r "type.*string" pkg/parser/schemas/ | grep engine`
3. Cross-reference with parser implementation

## Implementation Steps

### Step 1: Load Previous Strategies
```bash
# Check if strategies file exists
if [ -f /tmp/gh-aw/cache-memory/strategies.json ]; then
  cat /tmp/gh-aw/cache-memory/strategies.json
fi
```

### Step 2: Choose Strategy
- If cache exists and has strategies, use proven strategy 70% of time
- Otherwise or 30% of time, try new/different approach

### Step 3: Execute Analysis
Use chosen strategy to find inconsistencies. Examples:

**Example: Field enumeration**
```bash
# Extract schema fields using jq for robust JSON parsing
jq -r '.properties | keys[]' pkg/parser/schemas/main_workflow_schema.json 2>/dev/null | sort -u

# Extract struct fields from Rust crates (look for serde attributes)
grep -rn 'serde(rename' crates/*/src/*.rs | grep -o 'rename = "[^"]*"' | sort -u

# Extract struct field definitions from Rust crates
grep -rn 'pub [a-z_]*:' crates/*/src/*.rs | sort -u

# Extract documented fields
grep -r "^###\? " docs/src/content/docs/reference/frontmatter.md
```

**Example: Type checking**
```bash
# Find schema field types (handles different JSON Schema patterns)
jq -r '
  (.properties // {}) | to_entries[] | 
  "\(.key): \(.value.type // .value.oneOf // .value.anyOf // .value.allOf // "complex")"
' pkg/parser/schemas/main_workflow_schema.json 2>/dev/null || echo "Failed to parse schema"
```

### Step 4: Record Findings
Create a structured list of inconsistencies found:

```markdown
## Inconsistencies Found

### Schema ↔ Implementation Mismatches
1. **Field `engine.version`**: 
   - Schema: defines as string
   - Rust code: not validated in deserialization
   - Impact: Invalid values could pass through

### Schema ↔ Documentation Mismatches  
1. **Field `cache-memory`**:
   - Schema: defines array of objects with `id` and `key`
   - Docs: only shows simple boolean example
   - Impact: Advanced usage not documented

### Implementation ↔ Documentation Mismatches
1. **Error message for invalid `on` field**:
   - Rust code: "trigger configuration is required"
   - Docs: doesn't mention this error
   - Impact: Users may not understand error
```

### Step 5: Update Cache
Save successful strategy and findings to cache:
```bash
# Update strategies.json with results
cat > /tmp/gh-aw/cache-memory/strategies.json << 'EOF'
{
  "strategies": [...],
  "last_updated": "2024-XX-XX"
}
EOF
```

### Step 6: Create Issue
Generate a comprehensive report for issue output.

## Issue Report Format

Create a well-structured issue report:

```markdown
# 🔍 Schema Consistency Check - [DATE]

## Summary

- **Inconsistencies Found**: [NUMBER]
- **Categories Analyzed**: Schema, Parser, Documentation, Workflows
- **Strategy Used**: [STRATEGY NAME]
- **New Strategy**: [YES/NO]

## Critical Issues

[List high-priority inconsistencies that could cause bugs]

## Documentation Gaps

[List areas where docs don't match reality]

## Schema Improvements Needed

[List schema enhancements needed]

## Implementation Updates Required

[List Rust code that needs updates]

## Workflow Violations

[List workflows using invalid/undocumented features]

## Recommendations

1. [Specific actionable recommendation]
2. [Specific actionable recommendation]
3. [...]

## Strategy Performance

- **Strategy Used**: [NAME]
- **Findings**: [COUNT]
- **Effectiveness**: [HIGH/MEDIUM/LOW]
- **Should Reuse**: [YES/NO]

## Next Steps

- [ ] Fix schema definitions
- [ ] Update Rust validation code
- [ ] Update documentation
- [ ] Fix workflow files
```

## Important Guidelines

### Security
- Never execute untrusted code from workflows
- Validate all file paths before reading
- Sanitize all grep/bash commands
- Read-only access to schema, parser, and documentation files for analysis
- Only modify files in `/tmp/gh-aw/cache-memory/` (never modify source files)

### Quality
- Be thorough but focused on actionable findings
- Prioritize issues by severity (critical bugs vs documentation gaps)
- Provide specific file:line references when possible
- Include code snippets to illustrate issues
- Suggest concrete fixes

### Efficiency  
- Use bash tools efficiently (grep, jq, etc.)
- Cache results when re-analyzing same data
- Don't re-check things found in previous runs (check cache first)
- Focus on high-impact areas

### Strategy Evolution
- Try genuinely different approaches when not using cached strategies
- Document why a strategy worked or failed
- Update success metrics in cache
- Consider combining successful strategies

## Tools Available

You have access to:
- **bash**: Any command (use grep, jq, find, cat, etc.)
- **edit**: Create/modify files in cache memory
- **github**: Read repository data, issues

## Success Criteria

A successful run:
- ✅ Analyzes all 4 areas (schema, implementation, docs, workflows)
- ✅ Uses or creates an effective detection strategy
- ✅ Updates cache with strategy results
- ✅ Finds at least one category of inconsistencies OR confirms consistency
- ✅ Creates a detailed issue report
- ✅ Provides actionable recommendations

Begin your analysis now. Check the cache, choose a strategy, execute it, and report your findings in an issue.

**Important**: If no action is needed after completing your analysis, you **MUST** call the `noop` safe-output tool with a brief explanation. Failing to call any safe-output tool is the most common cause of safe-output workflow failures.

```json
{"noop": {"message": "No action needed: [brief explanation of what was analyzed and why]"}}
```
