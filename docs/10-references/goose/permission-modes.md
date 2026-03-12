# Permission Modes

Goose provides a robust permission system to secure tool usage.

## GooseMode
The global operating mode for an agent:
- **Auto**: High velocity, low security.
- **Approve**: Low velocity, high security.
- **SmartApprove**: Balanced approach. LLM determines read-only status and caches the result.

## Permission Levels
- **AlwaysAllow**: Permanent permission for a tool.
- **AskBefore**: User must approve each call.
- **NeverAllow**: Tool usage is strictly forbidden.

## Permission Manager
`crates/goose/src/config/permission.rs`
- Manages persistent settings in `permission.yaml`.
- Uses `ToolAnnotations` (specifically `read_only_hint`) to pre-classify tools.

## Implementation Details
- **PermissionInspector**: The engine that checks tool calls against active permissions.
- **PermissionJudge**: Uses a separate "fast" LLM call to determine if a tool is read-only if no hint is provided.
- **ToolPermissionStore**: Caches results based on tool name and argument hash (blake3).
