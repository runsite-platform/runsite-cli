mod api;
mod cli;
mod commands;
mod config;
mod error;
mod output;
mod tui;
mod ws;

use anyhow::Result;
use clap::Parser;
use cli::{
    Cli, Commands, ContextCommands, DbCommands, DeploymentCommands, EnvCommands, OutputFormat,
    ProjectCommands, ServiceCommands,
};
use std::sync::{Arc, Mutex};

/// Rust masks SIGPIPE at startup, so writing to a closed pipe raises an IO
/// error that `println!` turns into a panic. Restoring the default handler
/// makes `runsite service list | head` exit quietly like every other CLI.
#[cfg(unix)]
fn restore_default_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_default_sigpipe() {}

#[tokio::main]
async fn main() {
    restore_default_sigpipe();
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("{}: {}", "error".red(), err);
        std::process::exit(1);
    }
}

use colored::Colorize;

async fn run(cli: Cli) -> Result<()> {
    // Decided before the config is read, so a broken config never changes
    // what a bare `runsite` does outside a terminal.
    if cli.command.is_none() && (!tui::is_interactive() || matches!(cli.output, OutputFormat::Json))
    {
        use clap::CommandFactory;
        eprint!("{}", Cli::command().render_help());
        std::process::exit(2);
    }

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

    let Some(command) = cli.command else {
        return tui::run(config, profile_name).await;
    };

    match command {
        Commands::Ui => {
            if !tui::is_interactive() {
                anyhow::bail!("`runsite ui` needs an interactive terminal");
            }
            tui::run(config, profile_name).await?;
        }

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
            ServiceCommands::List { all } => commands::service::list(&client, all, format).await?,
            ServiceCommands::Create(args) => {
                commands::service::create(&client, &args, format).await?
            }
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

        Commands::Deployments(sub) => match sub {
            DeploymentCommands::List { service, limit } => {
                commands::deployments::list(&client, service.as_deref(), limit, format).await?
            }
            DeploymentCommands::Rollback {
                deployment,
                service,
            } => {
                commands::deployments::rollback(&client, service.as_deref(), &deployment, format)
                    .await?
            }
        },

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
            ProjectCommands::Create { name, description } => {
                commands::project::create(&client, &name, description, format).await?
            }
            ProjectCommands::Use { project } => {
                commands::project::use_project(&client, config, &profile_name, &project).await?
            }
            ProjectCommands::Unset => commands::project::unset_project(config, &profile_name)?,
        },

        Commands::Db(sub) => match sub {
            DbCommands::List => commands::database::list(&client, format).await?,
            DbCommands::Create {
                name,
                plan,
                engine,
                project,
            } => {
                commands::database::create(
                    &client,
                    &name,
                    &plan,
                    engine,
                    project.as_deref(),
                    format,
                )
                .await?
            }
            DbCommands::Start { database } => {
                commands::database::change_state(&client, &database, "start", format).await?
            }
            DbCommands::Stop { database } => {
                commands::database::change_state(&client, &database, "stop", format).await?
            }
            DbCommands::Delete { database, yes } => {
                commands::database::delete(&client, &database, yes).await?
            }
            DbCommands::Connect { database, service } => {
                commands::database::connect(&client, &database, service.as_deref(), format).await?
            }
        },

        Commands::Plans { service_type } => {
            commands::database::plans(&client, service_type, format).await?
        }

        Commands::Context(sub) => match sub {
            ContextCommands::Show => {
                commands::context::show(&client, config, &profile_name).await?
            }
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
