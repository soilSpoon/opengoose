use anyhow::Result;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_profiles::ProfileStore;

pub(super) fn run(name: &str, output: CliOutput) -> Result<()> {
    let store = ProfileStore::new()?;
    let profile = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.show",
            "profile": profile,
        }))?;
    } else {
        let yaml = profile.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}
