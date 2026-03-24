use std::collections::VecDeque;

use crate::tui::log_entry::LogEntry;

pub struct LogState {
    pub entries: VecDeque<LogEntry>,
    pub verbose: bool,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
}

impl LogState {
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= 1000 {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        if self.auto_scroll {
            self.scroll_offset = 0;
        }
    }

    pub fn visible(&self) -> Vec<&LogEntry> {
        if self.verbose {
            self.entries.iter().collect()
        } else {
            self.entries.iter().filter(|e| e.structured).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::App;
    use crate::tui::log_entry::LogEntry;
    use chrono::Utc;

    #[test]
    fn push_log_pops_front_when_at_limit() {
        let mut app = App::new();
        for i in 0..1000u64 {
            app.logs.entries.push_back(LogEntry {
                timestamp: Utc::now(),
                level: tracing::Level::INFO,
                target: format!("target-{i}"),
                message: format!("msg-{i}"),
                structured: true,
            });
        }
        assert_eq!(app.logs.entries.len(), 1000);
        app.push_log(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "new".into(),
            message: "new msg".into(),
            structured: true,
        });
        assert_eq!(app.logs.entries.len(), 1000);
        assert_eq!(
            app.logs
                .entries
                .back()
                .expect("deque should not be empty")
                .target,
            "new"
        );
    }

    #[test]
    fn push_log_does_not_reset_scroll_when_auto_scroll_false() {
        let mut app = App::new();
        app.logs.auto_scroll = false;
        app.logs.scroll_offset = 42;
        app.push_log(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "t".into(),
            message: "m".into(),
            structured: true,
        });
        assert_eq!(app.logs.scroll_offset, 42);
    }

    #[test]
    fn visible_logs_filters_by_verbose_flag() {
        let mut app = App::new();
        app.logs.entries.push_back(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::INFO,
            target: "t".into(),
            message: "structured".into(),
            structured: true,
        });
        app.logs.entries.push_back(LogEntry {
            timestamp: Utc::now(),
            level: tracing::Level::DEBUG,
            target: "t".into(),
            message: "unstructured".into(),
            structured: false,
        });

        app.logs.verbose = false;
        let visible = app.visible_logs();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].message, "structured");

        app.logs.verbose = true;
        let visible = app.visible_logs();
        assert_eq!(visible.len(), 2);
    }
}
