use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "runsite",
    about = "RunSite CLI — deploy and manage your services",
    version,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub output: OutputFormat,

    /// Config profile to use
    #[arg(long, global = true, env = "RUNSITE_PROFILE")]
    pub profile: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authenticate with RunSite
    Login {
        /// Use an API key from the dashboard instead of email + password
        #[arg(long)]
        token: Option<String>,
    },

    /// Clear saved credentials
    Logout,

    /// Show currently authenticated user
    Whoami,

    /// Manage web services
    #[command(subcommand)]
    Service(ServiceCommands),

    /// Trigger a new deployment
    Deploy {
        /// Service name or ID
        service: Option<String>,

        /// Follow build logs after triggering (dashboard only for now)
        #[arg(long, short)]
        watch: bool,
    },

    /// Show recent logs from a service
    Logs {
        /// Service name or ID
        service: Option<String>,

        /// Number of recent log lines to show
        #[arg(long, short, default_value = "100")]
        tail: u32,
    },

    /// Open an interactive shell in a service container (dashboard only for now)
    Shell {
        /// Service name or ID
        service: Option<String>,
    },

    /// Run a one-off command in a service container (dashboard only for now)
    Run {
        /// Service name or ID
        service: Option<String>,

        /// Command to run (after --)
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },

    /// Manage environment variables
    #[command(subcommand)]
    Env(EnvCommands),

    /// Manage projects
    #[command(subcommand)]
    Project(ProjectCommands),

    /// Show or update CLI context
    #[command(subcommand)]
    Context(ContextCommands),

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum ServiceCommands {
    /// List all web services
    List,

    /// Show service status
    Status {
        /// Service name or ID
        service: Option<String>,
    },

    /// Start a stopped service
    Start {
        /// Service name or ID
        service: Option<String>,
    },

    /// Stop a running service
    Stop {
        /// Service name or ID
        service: Option<String>,
    },

    /// Restart a service
    Restart {
        /// Service name or ID
        service: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum EnvCommands {
    /// List environment variables for a service
    List {
        /// Service name or ID
        service: Option<String>,
    },

    /// Set one or more environment variables (KEY=VALUE ...)
    Set {
        /// Service name or ID
        service: Option<String>,

        /// Variables in KEY=VALUE format
        #[arg(required = true)]
        vars: Vec<String>,
    },

    /// Delete one or more environment variables
    Delete {
        /// Service name or ID
        service: Option<String>,

        /// Variable names to delete
        #[arg(required = true)]
        keys: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List all projects
    List,

    /// Set the current project context
    Use {
        /// Project name or ID
        project: String,
    },
}

#[derive(Subcommand)]
pub enum ContextCommands {
    /// Show current context (API URL, project, user)
    Show,

    /// Set the API server URL
    SetUrl {
        /// API URL (e.g. https://api.runsite.app)
        url: String,
    },
}
