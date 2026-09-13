use clap::{ArgGroup, Args, Parser, Subcommand, ValueEnum};

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

    /// List deployments and roll back
    #[command(subcommand, visible_alias = "deployment")]
    Deployments(DeploymentCommands),

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
    /// List web services in the current project
    List {
        /// List services from every project, not just the selected one
        #[arg(long)]
        all: bool,
    },

    /// Create a web service from a git repository or a container image
    Create(Box<ServiceCreateArgs>),

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

#[derive(ValueEnum, Clone, Copy)]
pub enum ServiceType {
    WebService,
    StaticSite,
    Worker,
}

impl ServiceType {
    pub fn as_api_value(self) -> &'static str {
        match self {
            ServiceType::WebService => "web_service",
            ServiceType::StaticSite => "static_site",
            ServiceType::Worker => "worker",
        }
    }
}

#[derive(Args)]
#[command(group(ArgGroup::new("source").required(true).args(["repo", "image"])))]
pub struct ServiceCreateArgs {
    /// Service name
    pub name: String,

    /// Git repository URL to build from
    #[arg(long)]
    pub repo: Option<String>,

    /// Container image to run instead of building (e.g. nginx:1.27)
    #[arg(long, conflicts_with_all = ["branch", "build_command", "dockerfile", "root_dir", "auto_deploy"])]
    pub image: Option<String>,

    /// Git branch to deploy (default: main)
    #[arg(long)]
    pub branch: Option<String>,

    /// Project name or ID (default: the current project)
    #[arg(long)]
    pub project: Option<String>,

    /// Service type
    #[arg(long = "type", value_enum)]
    pub service_type: Option<ServiceType>,

    /// Port the app listens on (default: 8080)
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: Option<u16>,

    /// Build command
    #[arg(long)]
    pub build_command: Option<String>,

    /// Start command
    #[arg(long)]
    pub start_command: Option<String>,

    /// Path to the Dockerfile inside the repository
    #[arg(long)]
    pub dockerfile: Option<String>,

    /// Subdirectory of the repository to build from
    #[arg(long)]
    pub root_dir: Option<String>,

    /// Minimum number of instances (1-5)
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=5))]
    pub min_instances: Option<u8>,

    /// Maximum number of instances (1-5)
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=5))]
    pub max_instances: Option<u8>,

    /// Redeploy automatically on every push to the branch
    #[arg(long)]
    pub auto_deploy: bool,

    /// Environment variable in KEY=VALUE format, stored as a secret (repeatable)
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub env_vars: Vec<String>,
}

#[derive(Subcommand)]
pub enum DeploymentCommands {
    /// List recent deployments of a service
    List {
        /// Service name or ID
        service: Option<String>,

        /// Number of deployments to show (1-100)
        #[arg(long, short, default_value = "10", value_parser = clap::value_parser!(u8).range(1..=100))]
        limit: u8,
    },

    /// Roll a service back to a previous deployment
    Rollback {
        /// Deployment ID or its short prefix from `deployments list`
        deployment: String,

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

    /// Clear the current project context
    Unset,
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
