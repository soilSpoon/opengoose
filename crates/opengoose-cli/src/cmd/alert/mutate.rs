use anyhow::{Result, bail};

use opengoose_persistence::AlertStore;

pub(super) fn delete(store: &AlertStore, name: &str) -> Result<()> {
    if store.delete(name)? {
        println!("Deleted alert rule `{name}`.");
    } else {
        bail!("No alert rule named `{name}` found.");
    }
    Ok(())
}

pub(super) fn set_enabled(store: &AlertStore, name: &str, enabled: bool) -> Result<()> {
    let action = if enabled { "Enabled" } else { "Disabled" };
    if store.set_enabled(name, enabled)? {
        println!("{action} alert rule `{name}`.");
    } else {
        bail!("No alert rule named `{name}` found.");
    }
    Ok(())
}
