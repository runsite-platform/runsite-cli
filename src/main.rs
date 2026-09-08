mod api;
mod cli;
mod commands;
mod config;
mod error;
mod output;
mod ws;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, ContextCommands, EnvCommands, ProjectCommands, ServiceCommands};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("{}: {}", "error".red(), err);
        std::process::exit(1);
    }
}

use colored::Colorize;

async fn run(cli: Cli) -> Result<()> {
    let profile_override = cli.profile.as_deref();
    let (cfg, profile_name) = config::load(profile_override)?;
    let config = Arc::new(Mutex::new(cfg));

    let base_url = {
        let cfg = config.lock().unwrap();
        cfg.profiles
            .get(&profile_name)
            .cloned()
            .unwrap_or_default()
            .api_url
    };

    let client = api::ApiClient::new(base_url.clone(), config.clone(), profile_name.clone());
    let format = cli.output;

    match cli.command {
        Commands::Login { token } => {
            commands::auth::login(config, &profile_name, &base_url, token.as_deref()).await?;
        }

        Commands::Logout => {
            commands::auth::logout(config, &profile_name)?;
        }

        Commands::Whoami => {
            commands::auth::whoami(&client, format).await?;
        }

        Commands::Service(sub) => match sub {
            ServiceCommands::List => commands::service::list(&client, format).await?,
            ServiceCommands::Status { service } => {
                commands::service::status(&client, service.as_deref(), format).await?
            }
            ServiceCommands::Start { service } => {
                commands::service::start(&client, service.as_deref()).await?
            }
            ServiceCommands::Stop { service } => {
                commands::service::stop(&client, service.as_deref()).await?
            }
            ServiceCommands::Restart { service } => {
                commands::service::restart(&client, service.as_deref()).await?
            }
        },

        Commands::Deploy { service, watch } => {
            commands::deploy::trigger(&client, service.as_deref(), watch).await?;
        }

        Commands::Logs { service, tail } => {
            commands::logs::tail(&client, service.as_deref(), tail, format).await?;
        }

        Commands::Shell { service } => {
            commands::shell::interactive(&client, service.as_deref()).await?;
        }

        Commands::Run { service, command } => {
            commands::shell::run(&client, service.as_deref(), &command).await?;
        }

        Commands::Env(sub) => match sub {
            EnvCommands::List { service } => {
                commands::env::list(&client, service.as_deref(), format).await?
            }
            EnvCommands::Set { service, vars } => {
                commands::env::set(&client, service.as_deref(), &vars).await?
            }
            EnvCommands::Delete { service, keys } => {
                commands::env::delete(&client, service.as_deref(), &keys).await?
            }
        },

        Commands::Project(sub) => match sub {
            ProjectCommands::List => commands::project::list(&client, format).await?,
            ProjectCommands::Use { project } => {
                commands::project::use_project(&client, config, &profile_name, &project).await?
            }
        },

        Commands::Context(sub) => match sub {
            ContextCommands::Show => commands::context::show(config, &profile_name)?,
            ContextCommands::SetUrl { url } => {
                commands::context::set_url(config, &profile_name, &url)?
            }
        },

        Commands::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "runsite",
                &mut std::io::stdout(),
            );
        }
    }

    Ok(())
}
