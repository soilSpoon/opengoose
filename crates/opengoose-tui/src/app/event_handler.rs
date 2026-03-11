mod presentation;
mod reducer;
mod tick;

#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod reducer_tests;
#[cfg(test)]
mod tests_support;
#[cfg(test)]
mod tick_tests;

use std::time::Instant;

use opengoose_types::AppEvent;

use super::state::*;

impl App {
    pub fn push_event(&mut self, summary: &str, level: EventLevel) {
        self.events.push_back(EventEntry {
            summary: summary.to_string(),
            level,
            timestamp: Instant::now(),
        });
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        let (summary, level, notice) = presentation::summarize_event(&event.kind);
        reducer::apply(self, &event.kind);

        if let Some(notice) = notice {
            self.set_status_notice(notice, level);
        }

        if !reducer::shows_in_messages(&event.kind) {
            self.push_event(&summary, level);
        }
    }

    pub fn tick(&mut self) {
        tick::poll(self);
    }
}
