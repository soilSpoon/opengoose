use crate::data::views::SelectOption;

pub(super) fn build_team_options(
    installed_teams: &[String],
    selected_team: Option<&str>,
) -> Vec<SelectOption> {
    let mut names = installed_teams.to_vec();
    if let Some(selected_team) = selected_team
        && !selected_team.is_empty()
        && !names.iter().any(|team| team == selected_team)
    {
        names.push(selected_team.to_string());
        names.sort();
    }

    names
        .into_iter()
        .map(|team| SelectOption {
            selected: selected_team
                .map(|selected| selected == team.as_str())
                .unwrap_or(false),
            label: team.clone(),
            value: team,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::build_team_options;

    #[test]
    fn build_team_options_adds_missing_selection_in_sorted_order() {
        let options = build_team_options(&["alpha".into(), "zeta".into()], Some("beta"));

        assert_eq!(
            options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta", "zeta"]
        );
        assert_eq!(
            options
                .iter()
                .find(|option| option.value == "beta")
                .map(|option| option.selected),
            Some(true)
        );
    }
}
