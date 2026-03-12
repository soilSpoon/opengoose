# Dolt Reference

Dolt is a SQL database with Git-like versioning capabilities. While powerful, it was not chosen as the primary storage for OpenGoose due to deployment constraints.

## Why Dolt was not chosen

1. **Deployment Constraint**: Dolt requires a separate Go-based server process. OpenGoose's goal is a single-binary Rust application.
2. **Binary Size**: Including or requiring a Dolt server would add ~100MB to the deployment footprint.
3. **C-Dependencies**: OpenGoose aims for a pure-Rust stack to simplify cross-compilation and maintenance.

## When to use Dolt

Dolt remains a valid option for OpenGoose if:
- You need a full MySQL-compatible protocol.
- You require multi-instance synchronization via DoltHub or DoltLab.
- You are running 20+ agents where a dedicated database server provides better performance.

## Comparison with Prollytree

| Feature | Prollytree (Chosen) | Dolt |
|---------|---------------------|------|
| Runtime | Embedded (Rust) | Server (Go) |
| Merge | 3-way (Native) | 3-way (Cell-level) |
| SQL | GlueSQL | MySQL |
| Complexity | Low (Internal) | High (External) |
