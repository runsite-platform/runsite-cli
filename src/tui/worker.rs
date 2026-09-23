//! Runs effects against the API in background tasks and reports back as actions.

use super::action::{Action, FetchError, Payload, Request};
use super::log_buffer::LogQuery;
use crate::api::{ApiClient, Credentials};
use crate::config::{self, Config};
use anyhow::{bail, Result};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub struct Worker {
    client: ApiClient,
    config: Arc<Mutex<Config>>,
    profile: String,
    results: UnboundedSender<Action>,
}

impl Worker {
    pub fn new(
        config: Arc<Mutex<Config>>,
        profile: String,
        api_url: String,
        results: UnboundedSender<Action>,
    ) -> Self {
        let client = ApiClient::new(api_url, config.clone(), profile.clone());
        Self {
            client,
            config,
            profile,
            results,
        }
    }

    pub fn uses_env_token(&self) -> bool {
        self.client.uses_env_token()
    }

    pub fn fetch(&self, generation: u64, request: Request) {
        let client = self.client.clone();
        let results = self.results.clone();
        tokio::spawn(async move {
            let result = load(&client, request)
                .await
                .map_err(|error| FetchError::from_error(&error));
            let _ = results.send(Action::Loaded {
                generation,
                request,
                result,
            });
        });
    }

    pub fn fetch_logs(&self, generation: u64, service: Uuid, query: LogQuery) {
        let client = self.client.clone();
        let results = self.results.clone();
        tokio::spawn(async move {
            let result = client
                .service_logs(service, query.tail, query.since)
                .await
                .map(|body| Payload::Logs { body, query })
                .map_err(|error| FetchError::from_error(&error));
            let _ = results.send(Action::Loaded {
                generation,
                request: Request::Logs(service),
                result,
            });
        });
    }

    /// On success the key is saved to this profile, on disk and in memory.
    pub fn log_in(&self, generation: u64, credentials: Credentials) {
        let client = self.client.clone();
        let config = self.config.clone();
        let profile = self.profile.clone();
        let results = self.results.clone();
        tokio::spawn(async move {
            let result = log_in_and_store(&client, &config, &profile, &credentials)
                .await
                .map_err(|error| FetchError::from_error(&error));
            let _ = results.send(Action::LoggedIn { generation, result });
        });
    }
}

async fn load(client: &ApiClient, request: Request) -> Result<Payload> {
    Ok(match request {
        Request::CurrentUser => Payload::CurrentUser(client.current_user().await?),
        Request::KeyScope => Payload::KeyScope(client.current_key().await?),
        Request::Projects => Payload::Projects(client.project_summaries().await?),
        Request::ProjectDetail(id) => Payload::ProjectDetail(client.project_detail(id).await?),
        Request::UnassignedServices => Payload::UnassignedServices(client.services().await?),
        Request::Service(id) => Payload::Service(Box::new(client.service(id).await?)),
        Request::LatestDeployment(id) => Payload::LatestDeployment(
            client
                .deployments(id, 1)
                .await?
                .into_iter()
                .next()
                .map(Box::new),
        ),
        Request::Metrics(id) => Payload::Metrics(client.service_metrics(id).await?),
        Request::MetricsHistory(id) => {
            Payload::MetricsHistory(client.service_metrics_history(id, 3600).await?)
        }
        Request::Deployments(id) => Payload::Deployments(client.deployments(id, 10).await?),
        Request::Deployment(service, deployment) => {
            Payload::Deployment(Box::new(client.deployment(service, deployment).await?))
        }
        Request::Database(id) => Payload::Database(Box::new(client.database(id).await?)),
        Request::Logs(_) => bail!("log requests carry a query: use Worker::fetch_logs"),
    })
}

async fn log_in_and_store(
    client: &ApiClient,
    config: &Arc<Mutex<Config>>,
    profile: &str,
    credentials: &Credentials,
) -> Result<String> {
    let outcome = client.log_in(credentials).await?;
    let api_key = outcome.api_key;
    config::update_profile(profile, |saved| saved.api_key = Some(api_key.clone()))?;
    config
        .lock()
        .unwrap()
        .profiles
        .entry(profile.to_string())
        .or_default()
        .api_key = Some(api_key);
    Ok(outcome.email)
}
