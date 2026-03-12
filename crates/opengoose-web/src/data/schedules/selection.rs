use super::catalog::ScheduleCatalog;

pub(super) const NEW_SCHEDULE_KEY: &str = "__new__";

pub(super) enum Selection {
    Existing(String),
    New,
}

pub(super) fn resolve_selection(catalog: &ScheduleCatalog, selected: Option<String>) -> Selection {
    match selected.as_deref() {
        Some(NEW_SCHEDULE_KEY) => Selection::New,
        Some(target)
            if catalog
                .schedules
                .iter()
                .any(|schedule| schedule.name == target) =>
        {
            Selection::Existing(target.to_string())
        }
        _ => catalog
            .schedules
            .first()
            .map(|schedule| Selection::Existing(schedule.name.clone()))
            .unwrap_or(Selection::New),
    }
}
