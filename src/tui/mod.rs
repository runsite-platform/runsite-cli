mod action;
mod actions;
mod app;
mod dashboard;
mod detail;
mod detail_keys;
mod log_buffer;
mod login;
mod poller;
mod status;
mod theme;
mod views;
mod worker;

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod detail_tests;
#[cfg(test)]
mod fixtures;

use crate::config::{self, Config};
use action::{Action, Effect, ProfileEntry};
use anyhow::{bail, Result};
use app::{App, ExitReason, Session};
use chrono::Utc;
use futures_util::StreamExt;
use ratatui::crossterm::event::{Event, EventStream, KeyEventKind};
use std::io::IsTerminal;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use theme::Theme;
use tokio::sync::mpsc::{self, UnboundedSender};
use uuid::Uuid;
use worker::Worker;

const TICK: Duration = Duration::from_millis(250);

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn translate(event: Event) -> Option<Action> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => Some(Action::Key(key)),
        Event::Resize(width, height) => Some(Action::Resize { width, height }),
        _ => None,
    }
}

fn profile_entries(config: &Config) -> Vec<ProfileEntry> {
    let mut entries: Vec<ProfileEntry> = config
        .profiles
        .iter()
        .map(|(name, profile)| ProfileEntry {
            name: name.clone(),
            api_url: profile.api_url.clone(),
            has_key: profile.api_key.is_some(),
            current_project_id: profile
                .current_project_id
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok()),
        })
        .collect();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

struct Runtime {
    config: Arc<Mutex<Config>>,
    worker: Worker,
    results: UnboundedSender<Action>,
}

impl Runtime {
    /// Returns false once an `Exit` effect was seen.
    fn execute(&mut self, effects: Vec<Effect>) -> bool {
        for effect in effects {
            match effect {
                Effect::Fetch {
                    generation,
                    request,
                } => self.worker.fetch(generation, request),
                Effect::FetchLogs {
                    generation,
                    service,
                    query,
                } => self.worker.fetch_logs(generation, service, query),
                Effect::LogIn {
                    generation,
                    credentials,
                } => self.worker.log_in(generation, credentials),
                Effect::ListProfiles => {
                    let entries = config::read_current()
                        .map(|config| profile_entries(&config))
                        .unwrap_or_default();
                    let _ = self.results.send(Action::ProfilesListed(entries));
                }
                Effect::SwitchProfile { profile, api_url } => {
                    if let Ok(fresh) = config::read_current() {
                        *self.config.lock().unwrap() = fresh;
                    }
                    self.worker =
                        Worker::new(self.config.clone(), profile, api_url, self.results.clone());
                }
                Effect::Exit => return false,
            }
        }
        true
    }
}

#[cfg(unix)]
async fn termination_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    match (
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
    ) {
        (Ok(mut terminate), Ok(mut hangup)) => {
            tokio::select! {
                _ = terminate.recv() => {}
                _ = hangup.recv() => {}
            }
        }
        _ => std::future::pending().await,
    }
}

#[cfg(windows)]
async fn termination_signal() {
    match tokio::signal::windows::ctrl_close() {
        Ok(mut close) => {
            close.recv().await;
        }
        Err(_) => std::future::pending().await,
    }
}

pub async fn run(config: Arc<Mutex<Config>>, profile_name: String) -> Result<()> {
    let profile = config
        .lock()
        .unwrap()
        .profiles
        .get(&profile_name)
        .cloned()
        .unwrap_or_default();
    let preferred_project = profile
        .current_project_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok());

    let (results, mut incoming) = mpsc::unbounded_channel();
    let worker = Worker::new(
        config.clone(),
        profile_name.clone(),
        profile.api_url.clone(),
        results.clone(),
    );
    let env_token = worker.uses_env_token();
    let session = Session {
        profile: profile_name,
        api_url: profile.api_url,
        env_token,
        has_credentials: env_token || profile.api_key.is_some(),
        user: None,
        key_scope: None,
    };

    let theme = Theme::detect();
    let mut terminal = ratatui::try_init()?;
    let size = match terminal.size() {
        Ok(size) => size,
        Err(error) => {
            ratatui::restore();
            return Err(error.into());
        }
    };
    let mut app = App::new(
        session,
        preferred_project,
        Utc::now(),
        (size.width, size.height),
    );
    let mut runtime = Runtime {
        config,
        worker,
        results,
    };

    let outcome = event_loop(&mut terminal, &mut app, &mut runtime, &theme, &mut incoming).await;
    ratatui::restore();

    // Save every profile even if the loop or one of the writes failed.
    let mut save_error = None;
    for (profile, project_id) in &app.chosen_projects {
        let saved = config::update_profile(profile, |saved| {
            saved.current_project_id = Some(project_id.to_string())
        });
        if let Err(error) = saved {
            save_error.get_or_insert(error);
        }
    }
    outcome?;
    if app.exit == Some(ExitReason::EnvTokenRejected) {
        bail!("RUNSITE_API_TOKEN is invalid or revoked");
    }
    save_error.map_or(Ok(()), Err)
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    runtime: &mut Runtime,
    theme: &Theme,
    incoming: &mut mpsc::UnboundedReceiver<Action>,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    let termination = termination_signal();
    tokio::pin!(termination);

    if !runtime.execute(app.start()) {
        return Ok(());
    }
    loop {
        terminal.draw(|frame| views::render(frame, app, theme))?;
        let action = tokio::select! {
            Some(event) = events.next() => match translate(event?) {
                Some(action) => action,
                None => continue,
            },
            _ = ticker.tick() => Action::Tick { now: Utc::now() },
            Some(action) = incoming.recv() => action,
            _ = &mut termination => Action::Terminate,
        };
        if !runtime.execute(app::update(app, action)) {
            return Ok(());
        }
    }
}
