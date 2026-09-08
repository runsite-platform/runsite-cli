use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Auth

#[derive(Debug, Deserialize, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    #[serde(alias = "name")]
    pub full_name: Option<String>,
}

// Projects

#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectList {
    pub projects: Vec<Project>,
}

// Web Services

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebService {
    pub id: Uuid,
    pub name: String,
    pub status: Option<String>,
    pub url: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebServiceList {
    pub web_services: Vec<WebService>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebServiceDetail {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub url: Option<String>,
    pub subdomain: Option<String>,
    pub project_type: Option<String>,
    pub github_branch: Option<String>,
    pub github_repo_full_name: Option<String>,
    pub min_instances: Option<i32>,
    pub max_instances: Option<i32>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ContainerLogs {
    pub logs: String,
}

// Deployments

#[derive(Debug, Deserialize, Serialize)]
pub struct Deployment {
    pub id: Uuid,
    pub status: String,
}

// Environment Variables

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnvVar {
    pub id: Uuid,
    pub key: String,
    pub value: Option<String>,
    #[serde(default)]
    pub is_secret: bool,
}

// The public API wraps env vars in `environment_variables`, not `env_vars`.
#[derive(Debug, Deserialize, Serialize)]
pub struct EnvVarList {
    pub environment_variables: Vec<EnvVar>,
}

// `id` must carry the existing variable's UUID: the API rejects an id-less item
// whose key already exists with 409 Conflict.
#[derive(Debug, Serialize)]
pub struct BulkEnvVarItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    pub key: String,
    pub value: String,
    pub is_secret: bool,
}

// PUT /env/bulk expects the list under `variables`.
#[derive(Debug, Serialize)]
pub struct BulkEnvVars {
    pub variables: Vec<BulkEnvVarItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // These payloads mirror the public API's response models. They exist because
    // the wrapper keys drifted once already: `environment_variables` was read as
    // `env_vars`, and the bulk body was sent as `env_vars` instead of `variables`.

    #[test]
    fn env_var_list_reads_the_environment_variables_key() {
        let json = r#"{
            "environment_variables": [
                {"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "web_service_id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b",
                 "key": "DATABASE_URL", "value": "postgres://x", "is_secret": true,
                 "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"}
            ],
            "total": 1
        }"#;

        let parsed: EnvVarList = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.environment_variables.len(), 1);
        assert_eq!(parsed.environment_variables[0].key, "DATABASE_URL");
        assert!(parsed.environment_variables[0].is_secret);
    }

    #[test]
    fn bulk_body_wraps_items_in_variables() {
        let body = BulkEnvVars {
            variables: vec![BulkEnvVarItem {
                id: None,
                key: "PORT".to_string(),
                value: "8080".to_string(),
                is_secret: false,
            }],
        };

        let json: serde_json::Value = serde_json::to_value(&body).unwrap();
        assert!(json.get("variables").is_some());
        // An absent id means "create"; sending `null` would be rejected as a lookup.
        assert!(json["variables"][0].get("id").is_none());
    }

    #[test]
    fn bulk_body_keeps_the_id_of_an_existing_variable() {
        let id = Uuid::parse_str("8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b").unwrap();
        let body = BulkEnvVars {
            variables: vec![BulkEnvVarItem {
                id: Some(id),
                key: "PORT".to_string(),
                value: "8080".to_string(),
                is_secret: false,
            }],
        };

        let json: serde_json::Value = serde_json::to_value(&body).unwrap();
        assert_eq!(json["variables"][0]["id"], id.to_string());
    }

    #[test]
    fn user_response_accepts_the_name_field() {
        let json =
            r#"{"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "email": "a@b.c", "name": "Ada"}"#;
        let parsed: UserResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.full_name.as_deref(), Some("Ada"));
    }

    #[test]
    fn web_service_list_tolerates_a_service_without_a_url() {
        let json = r#"{
            "web_services": [
                {"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "name": "api", "status": "stopped",
                 "url": null, "updated_at": "2026-01-01T00:00:00Z", "project_id": null}
            ],
            "total": 1
        }"#;

        let parsed: WebServiceList = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.web_services[0].url, None);
    }

    #[test]
    fn web_service_detail_reads_the_full_response() {
        let json = r#"{
            "id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "name": "api", "status": "running",
            "url": "https://api-x.runsite.app", "subdomain": "api-x", "project_type": "web_service",
            "github_branch": "main", "github_repo_full_name": "me/api",
            "min_instances": 1, "max_instances": 3, "updated_at": "2026-01-01T00:00:00Z"
        }"#;

        let parsed: WebServiceDetail = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.status, "running");
        assert_eq!(parsed.max_instances, Some(3));
    }
}
