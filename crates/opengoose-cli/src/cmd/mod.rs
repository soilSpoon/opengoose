/// Monitoring alert rule management (`opengoose alert`).
pub mod alert;
/// AI provider authentication and credential management (`opengoose auth`).
pub mod auth;
/// Inter-agent messaging commands (`opengoose message`).
pub mod message;
/// CLI output formatting helpers (text tables, JSON, error display).
pub mod output;
/// Plugin lifecycle management (`opengoose plugin`).
pub mod plugin;
/// Agent profile management (`opengoose profile`).
pub mod profile;
/// Remote agent connection management (`opengoose remote`).
pub mod remote;
/// Main runtime entry point (`opengoose run`).
pub mod run;
/// Cron schedule management (`opengoose schedule`).
pub mod schedule;
/// Skill package management (`opengoose skill`).
pub mod skill;
/// Team definition and execution management (`opengoose team`).
pub mod team;
/// Event trigger management (`opengoose trigger`).
pub mod trigger;
/// Web dashboard server (`opengoose web`).
pub mod web;
