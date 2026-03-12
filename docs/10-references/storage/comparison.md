# Storage Comparison Table

A comparison of storage options evaluated for OpenGoose.

| Feature | SQLite + Diesel | **prollytree** | Dolt |
|---------|:--------------:|:--------------:|:----:|
| Single Binary | ✅ | ✅ | ❌ (Go Server) |
| Pure Rust | ❌ (C-lib) | ✅ | ❌ |
| Structural Sharing | ❌ | ✅ | ✅ |
| O(diff) Complexity | ❌ | ✅ | ✅ |
| 3-way Merge | Custom | ✅ Built-in | ✅ |
| Git Integration | ❌ | ✅ | ✅ |
| SQL Support | ✅ Full | ✅ GlueSQL | ✅ MySQL |
| Deployment | Simple | Simple | Complex |

## Decision: `prollytree`
Chosen for its pure-Rust implementation, native 3-way merge support, and alignment with the single-binary requirement while providing the structural efficiency needed for high-scale agent orchestration.
