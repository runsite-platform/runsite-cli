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

#[derive(Debug, Serialize)]
pub struct ProjectCreate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// Plans

#[derive(Debug, Deserialize, Serialize)]
pub struct ServicePlan {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub monthly_price_cents: i64,
    pub cpu_limit: Option<f64>,
    pub memory_limit_mb: Option<i64>,
    pub storage_limit_gb: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServicePlanList {
    pub plans: Vec<ServicePlan>,
}

// Managed databases (Postgres and Valkey). The public API never returns credentials.

#[derive(Debug, Deserialize, Serialize)]
pub struct ManagedDatabase {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub plan_name: Option<String>,
    pub storage_limit_gb: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ManagedDatabaseList {
    pub databases: Vec<ManagedDatabase>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DatabaseStatus {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseCreate {
    pub name: String,
    pub plan_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ConnectionCreate {
    pub target_type: String,
    pub target_id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Connection {
    pub id: Uuid,
    pub target_type: String,
    pub env_var_key: String,
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

#[derive(Debug, Serialize)]
pub struct EnvVarCreate {
    pub key: String,
    pub value: String,
    pub is_secret: bool,
}

// Only fields the user chose are sent, so the API applies its own defaults
// (port 8080, one instance, branch `main`, ...) to everything else.
#[derive(Debug, Serialize, Default)]
pub struct WebServiceCreate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_repository_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dockerfile_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_instances: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_instances: Option<u8>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub auto_deploy: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<EnvVarCreate>,
}

// Deployments

#[derive(Debug, Deserialize, Serialize)]
pub struct Deployment {
    pub id: Uuid,
    pub status: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    #[serde(default)]
    pub is_live: bool,
    pub trigger_type: Option<String>,
    pub error_message: Option<String>,
    pub rollback_from_deployment_id: Option<Uuid>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeploymentList {
    pub deployments: Vec<Deployment>,
    pub total: i64,
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

// Views used by the interactive TUI. They are kept apart from the types above
// so that the `--output json` shape of existing commands never changes.

#[derive(Debug, Deserialize, Clone, PartialEq)]
/// `active_workspace_name` is not read: the API leaves it null for API keys.
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ApiKeyIdentity {
    pub id: Uuid,
    pub name: String,
    pub scope: String,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct ServiceSummary {
    #[serde(default)]
    pub web_services: i64,
    #[serde(default)]
    pub postgresql: i64,
    #[serde(default)]
    pub redis: i64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub service_summary: ServiceSummary,
}

#[derive(Debug, Deserialize)]
pub struct ProjectSummaryList {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ServiceInfo {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub url: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_type: Option<String>,
    pub source_type: Option<String>,
    pub image_ref: Option<String>,
    pub github_branch: Option<String>,
    pub min_instances: Option<i32>,
    pub max_instances: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceInfoList {
    pub web_services: Vec<ServiceInfo>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct PostgresInProject {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub postgres_version: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct RedisInProject {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub redis_version: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProjectDetail {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub web_services_list: Vec<ServiceInfo>,
    #[serde(default)]
    pub databases_list: Vec<PostgresInProject>,
    #[serde(default)]
    pub redis_list: Vec<RedisInProject>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ResourceMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: i64,
    pub memory_limit_bytes: i64,
    pub memory_usage_percent: f64,
    pub instance_count: i64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct MetricsPoint {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
}

#[derive(Debug, Deserialize)]
pub struct MetricsHistory {
    pub data_points: Vec<MetricsPoint>,
}

/// A deployment with its build logs; the list endpoint includes them too.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct DeploymentInfo {
    pub id: Uuid,
    pub status: String,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub image_ref: Option<String>,
    #[serde(default)]
    pub is_live: bool,
    pub build_logs: Option<String>,
    pub error_message: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct DeploymentInfoList {
    pub deployments: Vec<DeploymentInfo>,
}

/// `GET /databases/{id}`: CPU and memory are only reported for Postgres.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct DatabaseDetail {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub plan_name: Option<String>,
    pub internal_hostname: Option<String>,
    pub external_hostname: Option<String>,
    pub cpu_usage_percent: Option<f64>,
    pub memory_usage_percent: Option<f64>,
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
    fn database_create_omits_an_absent_project() {
        let body = DatabaseCreate {
            name: "main".to_string(),
            plan_id: Uuid::parse_str("8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b").unwrap(),
            project_id: None,
        };

        let json: serde_json::Value = serde_json::to_value(&body).unwrap();
        assert!(json.get("project_id").is_none());
    }

    #[test]
    fn database_list_reads_both_engines() {
        let json = r#"{
            "databases": [
                {"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "kind": "valkey", "name": "cache",
                 "status": "running", "plan_name": null, "storage_limit_gb": 0,
                 "created_at": "2026-01-01T00:00:00Z"}
            ],
            "total": 1
        }"#;

        let parsed: ManagedDatabaseList = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.databases[0].kind, "valkey");
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
    fn deployment_list_reads_the_deployments_key() {
        let json = r#"{
            "deployments": [
                {"id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "web_service_id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b",
                 "commit_sha": "a1b2c3d4e5f6", "commit_message": "fix: login", "branch": "main",
                 "status": "running", "is_live": true, "build_logs": null, "error_message": null,
                 "started_at": null, "finished_at": null, "trigger_type": "manual",
                 "created_at": "2026-01-01T00:00:00Z"}
            ],
            "total": 1
        }"#;

        let parsed: DeploymentList = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.total, 1);
        assert!(parsed.deployments[0].is_live);
        assert_eq!(parsed.deployments[0].branch.as_deref(), Some("main"));
    }

    #[test]
    fn a_minimal_create_body_sends_only_the_name() {
        let body = WebServiceCreate {
            name: "api".to_string(),
            ..Default::default()
        };

        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json, serde_json::json!({ "name": "api" }));
    }

    #[test]
    fn a_create_body_carries_the_chosen_fields() {
        let body = WebServiceCreate {
            name: "api".to_string(),
            git_repository_url: Some("https://github.com/me/api".to_string()),
            port: Some(3000),
            auto_deploy: true,
            env_vars: vec![EnvVarCreate {
                key: "PORT".to_string(),
                value: "3000".to_string(),
                is_secret: true,
            }],
            ..Default::default()
        };

        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["port"], 3000);
        assert_eq!(json["auto_deploy"], true);
        assert_eq!(json["env_vars"][0]["key"], "PORT");
        assert!(json.get("image_ref").is_none());
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
