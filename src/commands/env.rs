use super::parse_key_value;
use crate::api::{ApiClient, BulkEnvVarItem, BulkEnvVars, EnvVar};
use crate::output::{write_table, Report};
use anyhow::{bail, Result};
use serde::Serialize;
use std::io::{self, Write};
use tabled::Tabled;
use uuid::Uuid;

#[derive(Tabled)]
struct EnvRow {
    #[tabled(rename = "KEY")]
    key: String,
    #[tabled(rename = "VALUE")]
    value: String,
}

impl From<&EnvVar> for EnvRow {
    fn from(variable: &EnvVar) -> Self {
        Self {
            key: variable.key.clone(),
            value: variable.value.clone().unwrap_or_else(|| "***".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct EnvVars(pub Vec<EnvVar>);

impl Report for EnvVars {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        write_table(out, self.0.iter().map(EnvRow::from).collect())
    }
}

/// The service's variables after `env set`.
#[derive(Debug, Serialize)]
pub struct EnvVarsSet {
    #[serde(skip)]
    pub set_count: usize,
    pub variables: Vec<EnvVar>,
}

impl Report for EnvVarsSet {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "{} variable(s) set.", self.set_count)
    }
}

#[derive(Debug, Serialize)]
pub struct EnvVarsDeleted {
    pub service_id: Uuid,
    pub action: &'static str,
    pub keys: Vec<String>,
}

impl Report for EnvVarsDeleted {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        for key in &self.keys {
            writeln!(out, "Deleted: {}", key)?;
        }
        Ok(())
    }
}

pub async fn list(client: &ApiClient, service: Option<&str>) -> Result<EnvVars> {
    let id = client.resolve_service_id(service).await?;
    Ok(EnvVars(client.env_vars(id).await?))
}

pub async fn set(client: &ApiClient, service: Option<&str>, vars: &[String]) -> Result<EnvVarsSet> {
    let id = client.resolve_service_id(service).await?;
    let existing = client.env_vars(id).await?;
    let body = BulkEnvVars {
        variables: upsert_items(&existing, vars)?,
    };
    let result = client.set_env_vars(id, &body).await?;
    Ok(EnvVarsSet {
        set_count: vars.len(),
        variables: result.environment_variables,
    })
}

/// An existing key keeps its id and secrecy; a new key is secret by default.
fn upsert_items(existing: &[EnvVar], vars: &[String]) -> Result<Vec<BulkEnvVarItem>> {
    vars.iter()
        .map(|raw| {
            let (key, value) = parse_key_value(raw)?;
            let current = existing.iter().find(|variable| variable.key == key);
            Ok(BulkEnvVarItem {
                id: current.map(|variable| variable.id),
                key: key.to_string(),
                value: Some(value.to_string()),
                is_secret: current.map(|variable| variable.is_secret).unwrap_or(true),
                deleted: false,
            })
        })
        .collect()
}

/// Every key is checked first, then all are deleted in one request: a typo or
/// a failure deletes nothing.
pub async fn delete(
    client: &ApiClient,
    service: Option<&str>,
    keys: &[String],
) -> Result<EnvVarsDeleted> {
    let id = client.resolve_service_id(service).await?;
    let existing = client.env_vars(id).await?;

    let missing: Vec<&str> = keys
        .iter()
        .filter(|key| !existing.iter().any(|variable| &variable.key == *key))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        bail!(
            "variable(s) not found: {}; nothing was deleted",
            missing.join(", ")
        );
    }

    let body = BulkEnvVars {
        variables: existing
            .iter()
            .filter(|variable| keys.contains(&variable.key))
            .map(BulkEnvVarItem::deletion)
            .collect(),
    };
    client.set_env_vars(id, &body).await?;

    Ok(EnvVarsDeleted {
        service_id: id,
        action: "delete",
        keys: keys.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::*;
    use crate::output::{render_json, render_text};
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const VARIABLE_ID: &str = "3f2a1b4c-0000-4000-8000-0000000000e1";

    fn env_route() -> String {
        format!("/api/v1/web-services/{SERVICE_ID}/env")
    }

    async fn serve_existing(server: &MockServer) {
        serve(
            server,
            &env_route(),
            serde_json::json!({"environment_variables": [
                {"id": VARIABLE_ID, "key": "PORT", "value": "8080", "is_secret": false}
            ]}),
        )
        .await;
    }

    fn variable(key: &str, is_secret: bool) -> EnvVar {
        EnvVar {
            id: uuid(VARIABLE_ID),
            key: key.to_string(),
            value: None,
            is_secret,
        }
    }

    #[test]
    fn an_existing_key_keeps_its_id_and_secrecy() {
        let items = upsert_items(&[variable("PORT", false)], &["PORT=9090".to_string()]).unwrap();
        assert_eq!(items[0].id, Some(uuid(VARIABLE_ID)));
        assert!(!items[0].is_secret);
    }

    #[test]
    fn a_new_key_is_secret() {
        let items = upsert_items(&[], &["TOKEN=abc=def".to_string()]).unwrap();
        assert_eq!(items[0].id, None);
        assert_eq!(items[0].value.as_deref(), Some("abc=def"));
        assert!(items[0].is_secret);
    }

    #[test]
    fn a_value_without_equals_is_rejected() {
        assert!(upsert_items(&[], &["TOKEN".to_string()]).is_err());
    }

    #[tokio::test]
    async fn set_reports_the_variables_after_the_change() {
        let server = MockServer::start().await;
        serve_existing(&server).await;
        expect_request(
            &server,
            "PUT",
            &format!("{}/bulk", env_route()),
            200,
            serde_json::json!({
                "environment_variables": [
                    {"id": VARIABLE_ID, "key": "PORT", "value": null, "is_secret": false}
                ],
                "total": 1, "created": 0, "updated": 1, "deleted": 0
            }),
        )
        .await;

        let report = set(
            &client_for(&server),
            Some(SERVICE_ID),
            &["PORT=9090".to_string()],
        )
        .await
        .unwrap();

        assert_eq!(render_json(&report)["variables"][0]["key"], "PORT");
        assert_eq!(render_text(&report), "1 variable(s) set.\n");
    }

    #[tokio::test]
    async fn delete_with_an_unknown_key_deletes_nothing() {
        let server = MockServer::start().await;
        serve_existing(&server).await;
        Mock::given(method("PUT"))
            .and(path(format!("{}/bulk", env_route())))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let error = delete(
            &client_for(&server),
            Some(SERVICE_ID),
            &["PORT".to_string(), "MISSING".to_string()],
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("MISSING"));
    }

    #[tokio::test]
    async fn delete_reports_the_deleted_keys() {
        let server = MockServer::start().await;
        serve_existing(&server).await;
        Mock::given(method("PUT"))
            .and(path(format!("{}/bulk", env_route())))
            .and(body_json(serde_json::json!({
                "variables": [
                    {"id": VARIABLE_ID, "key": "PORT", "is_secret": false, "deleted": true}
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "environment_variables": [], "total": 0, "created": 0, "updated": 0, "deleted": 1
            })))
            .expect(1)
            .mount(&server)
            .await;

        let report = delete(
            &client_for(&server),
            Some(SERVICE_ID),
            &["PORT".to_string()],
        )
        .await
        .unwrap();

        assert_eq!(
            render_json(&report),
            serde_json::json!({"service_id": SERVICE_ID, "action": "delete", "keys": ["PORT"]})
        );
        assert_eq!(render_text(&report), "Deleted: PORT\n");
    }
}
