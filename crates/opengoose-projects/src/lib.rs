//! Project definitions and context for OpenGoose agent orchestration.
//!
//! A [`ProjectDefinition`] is a YAML-backed configuration that binds a set of
//! agents and teams to a shared **goal**, a dedicated working directory
//! (`cwd`), and optional context files.  Agents running inside a project
//! always see the project goal and context in their system prompt, and file
//! operations are scoped to the project's `cwd`.
//!
//! The [`ProjectStore`] manages project files on disk (`~/.opengoose/projects/`)
//! using the same YAML-file-store pattern as [`opengoose_profiles`] and
//! [`opengoose_teams`].
//!
//! The [`ProjectContext`] is the runtime representation: a fully-resolved
//! struct created from a `ProjectDefinition` with context files already loaded
//! from disk.  It is cheap to clone and safe to share across async tasks via
//! `Arc<ProjectContext>`.

mod error;
mod project;
mod store;

pub use error::{ProjectError, ProjectResult};
pub use project::{ProjectContext, ProjectDefinition, ProjectSettings};
pub use store::ProjectStore;
