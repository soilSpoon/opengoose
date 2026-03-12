use diesel::prelude::*;

use crate::schema::{alert_history, alert_rules};

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = alert_rules)]
pub struct AlertRuleRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metric: String,
    pub condition: String,
    pub threshold: f64,
    pub enabled: i32,
    pub actions: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = alert_rules)]
pub struct NewAlertRule<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub metric: &'a str,
    pub condition: &'a str,
    pub threshold: f64,
    pub actions: &'a str,
}

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = alert_history)]
pub struct AlertHistoryRow {
    pub id: i32,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub triggered_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = alert_history)]
pub struct NewAlertHistory<'a> {
    pub rule_id: &'a str,
    pub rule_name: &'a str,
    pub metric: &'a str,
    pub value: f64,
}
