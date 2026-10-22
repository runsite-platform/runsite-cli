mod api;
mod cli;
mod commands;
mod config;
mod error;
mod output;
mod tui;
mod update;

/// Sent with every HTTP request: the platform API and GitHub releases.
pub const USER_AGENT: &str = concat!("runsite-cli/", env!("CARGO_PKG_VERSION"));

use anyhow::Result;
use api::{DatabaseCommand, ServiceCommand};
use clap::Parser;
use cli::{
    Cli, Commands, ContextCommands, DbCommands, DeploymentCommands, EnvCommands, OutputFormat,
    ProjectCommands, ServiceCommands,
};
use output::emit;

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
    let notifier = if wants_update_notice(&cli) {
        update::notice::UpdateNotifier::start()
    } else {
        update::notice::UpdateNotifier::disabled()
    };
    let result = run(cli).await;
    notifier.finish().await;
    if let Err(err) = result {
        eprintln!("{}: {}", "error".red(), err);
        std::process::exit(1);
    }
}

fn wants_update_notice(cli: &Cli) -> bool {
    let quiet_command = matches!(
        cli.command,
        Some(Commands::Upgrade { .. } | Commands::Completions { .. })
    );
    !quiet_command && update::notice::should_check(matches!(cli.output, OutputFormat::Json))
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

    // Needs no config, so a broken config.toml can still be fixed by upgrading.
    if let Some(Commands::Upgrade { check }) = cli.command {
        return update::install::upgrade(check, cli.output).await;
    }

    let profile = config::ActiveProfile::load(cli.profile.as_deref())?;
    let client = api::ApiClient::new(profile.clone());
    let format = cli.output;

    let Some(command) = cli.command else {
        return tui::run(profile).await;
    };

    match command {
        Commands::Ui => {
            if !tui::is_interactive() {
                anyhow::bail!("`runsite ui` needs an interactive terminal");
            }
            tui::run(profile).await?;
        }

        Commands::Login { token } => emit(
            format,
            &commands::auth::login(&client, &profile, token.as_deref()).await?,
        )?,

        Commands::Logout => emit(format, &commands::auth::logout(&profile)?)?,

        Commands::Whoami => emit(format, &commands::auth::whoami(&client).await?)?,

        Commands::Service(sub) => match sub {
            ServiceCommands::List { all } => {
                emit(format, &commands::service::list(&client, all).await?)?
            }
            ServiceCommands::Create(args) => {
                emit(format, &commands::service::create(&client, &args).await?)?
            }
            ServiceCommands::Status { service } => emit(
                format,
                &commands::service::status(&client, service.as_deref()).await?,
            )?,
            ServiceCommands::Start { service } => emit(
                format,
                &commands::service::run_command(&client, service.as_deref(), ServiceCommand::Start)
                    .await?,
            )?,
            ServiceCommands::Stop { service } => emit(
                format,
                &commands::service::run_command(&client, service.as_deref(), ServiceCommand::Stop)
                    .await?,
            )?,
            ServiceCommands::Restart { service } => emit(
                format,
                &commands::service::run_command(
                    &client,
                    service.as_deref(),
                    ServiceCommand::Restart,
                )
                .await?,
            )?,
        },

        Commands::Deploy { service, watch } => emit(
            format,
            &commands::deploy::trigger(&client, service.as_deref(), watch).await?,
        )?,

        Commands::Deployments(sub) => match sub {
            DeploymentCommands::List { service, limit } => emit(
                format,
                &commands::deployments::list(&client, service.as_deref(), limit).await?,
            )?,
            DeploymentCommands::Rollback {
                deployment,
                service,
            } => emit(
                format,
                &commands::deployments::rollback(&client, service.as_deref(), &deployment).await?,
            )?,
        },

        Commands::Logs { service, tail } => emit(
            format,
            &commands::logs::tail(&client, service.as_deref(), tail).await?,
        )?,

        Commands::Shell { service } => commands::shell::interactive(service.as_deref())?,

        Commands::Run { service, command } => commands::shell::run(service.as_deref(), &command)?,

        Commands::Env(sub) => match sub {
            EnvCommands::List { service } => emit(
                format,
                &commands::env::list(&client, service.as_deref()).await?,
            )?,
            EnvCommands::Set { service, vars } => emit(
                format,
                &commands::env::set(&client, service.as_deref(), &vars).await?,
            )?,
            EnvCommands::Delete { service, keys } => emit(
                format,
                &commands::env::delete(&client, service.as_deref(), &keys).await?,
            )?,
        },

        Commands::Project(sub) => match sub {
            ProjectCommands::List => emit(format, &commands::project::list(&client).await?)?,
            ProjectCommands::Create { name, description } => emit(
                format,
                &commands::project::create(&client, &name, description).await?,
            )?,
            ProjectCommands::Use { project } => emit(
                format,
                &commands::project::use_project(&client, &profile, &project).await?,
            )?,
            ProjectCommands::Unset => emit(format, &commands::project::unset_project(&profile)?)?,
        },

        Commands::Db(sub) => match sub {
            DbCommands::List => emit(format, &commands::database::list(&client).await?)?,
            DbCommands::Create {
                name,
                plan,
                engine,
                project,
            } => emit(
                format,
                &commands::database::create(&client, &name, &plan, engine, project.as_deref())
                    .await?,
            )?,
            DbCommands::Start { database } => emit(
                format,
                &commands::database::change_state(&client, &database, DatabaseCommand::Start)
                    .await?,
            )?,
            DbCommands::Stop { database } => emit(
                format,
                &commands::database::change_state(&client, &database, DatabaseCommand::Stop)
                    .await?,
            )?,
            DbCommands::Delete { database, yes } => emit(
                format,
                &commands::database::delete(&client, &database, yes).await?,
            )?,
            DbCommands::Connect { database, service } => emit(
                format,
                &commands::database::connect(&client, &database, service.as_deref()).await?,
            )?,
        },

        Commands::Plans { service_type } => emit(
            format,
            &commands::database::plans(&client, service_type).await?,
        )?,

        Commands::Context(sub) => match sub {
            ContextCommands::Show => {
                emit(format, &commands::context::show(&client, &profile).await?)?
            }
            ContextCommands::SetUrl { url } => {
                emit(format, &commands::context::set_url(&profile, &url)?)?
            }
        },

        Commands::Upgrade { .. } => unreachable!("handled before the config is loaded"),

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
