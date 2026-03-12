use anyhow::Result;
use opengoose_teams::{TeamDefinition, TeamStore, all_defaults as default_teams};

#[derive(Clone)]
pub(super) struct TeamCatalogEntry {
    pub(super) name: String,
    pub(super) team: TeamDefinition,
    pub(super) source_label: String,
    pub(super) is_live: bool,
}

pub(super) fn load_teams_catalog() -> Result<Vec<TeamCatalogEntry>> {
    let store = TeamStore::new()?;
    let names = store.list()?;
    if names.is_empty() {
        return Ok(default_teams()
            .into_iter()
            .map(|team| TeamCatalogEntry {
                name: team.name().to_string(),
                team,
                source_label: "Bundled default".into(),
                is_live: false,
            })
            .collect());
    }

    names
        .into_iter()
        .map(|name| {
            let team = store.get(&name)?;
            Ok(TeamCatalogEntry {
                name,
                team,
                source_label: store.dir().display().to_string(),
                is_live: true,
            })
        })
        .collect()
}
