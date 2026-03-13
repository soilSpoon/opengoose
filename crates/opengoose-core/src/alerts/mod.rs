//! Alert dispatcher: evaluates alert rules against system metrics and fires actions.

mod dispatcher;
mod types;

#[cfg(test)]
mod tests;

pub use dispatcher::AlertDispatcher;
pub use types::AlertDispatchError;
