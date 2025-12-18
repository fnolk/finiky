use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod config;
mod dhcp;
mod filesystem;
mod http;
mod server;
mod tftp;

const DEFAULT_CONFIG: &str = include_str!("../examples/config.toml");

#[derive(Parser)]
#[command(name = "finiky")]
#[command(about = "PXE Server for rapid OS deployment to bare metal")]
struct Cli {
    /// Enable debug mode (use multiple times for more verbosity: -d = debug, -dd = trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate the default configuration file
    GenConfig {
        /// Output file path (default: config.toml)
        #[arg(default_value = "config.toml")]
        file: PathBuf,
    },
    /// Start the PXE server
    Start {
        /// Path to configuration file
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// DHCP server port
        #[arg(long)]
        dhcp_port: Option<u16>,

        /// TFTP server port
        #[arg(long)]
        tftp_port: Option<u16>,

        /// HTTP server port
        #[arg(long)]
        http_port: Option<u16>,

        /// TFTP root directory or tar.gz file
        #[arg(long)]
        tftp_root: Option<PathBuf>,

        /// HTTP root directory or tar.gz file
        #[arg(long)]
        http_root: Option<PathBuf>,

        /// Network interface to bind to
        #[arg(long)]
        interface: Option<String>,

        /// Enable EFI protocol
        #[arg(long)]
        enable_efi: Option<bool>,

        /// Enable Legacy protocol
        #[arg(long)]
        enable_legacy: Option<bool>,

        /// Enable DHCP-boot protocol
        #[arg(long)]
        enable_dhcp_boot: Option<bool>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging based on debug flag count
    let log_level = match cli.debug {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    match cli.command {
        Some(Commands::GenConfig { file }) => {
            std::fs::write(&file, DEFAULT_CONFIG)?;
            println!("Configuration written to: {}", file.display());
            return Ok(());
        }
        Some(Commands::Start {
            config: config_path,
            dhcp_port,
            tftp_port,
            http_port,
            tftp_root,
            http_root,
            interface,
            enable_efi,
            enable_legacy,
            enable_dhcp_boot,
        }) => {
            let mut config = if let Some(config_path) = config_path {
                config::Config::from_file(&config_path)?
            } else {
                config::Config::default()
            };

            // Apply command line overrides
            if let Some(port) = dhcp_port {
                config.dhcp.port = port;
            }
            if let Some(port) = tftp_port {
                config.tftp.port = port;
            }
            if let Some(port) = http_port {
                config.http.port = port;
            }
            if let Some(root) = tftp_root {
                config.tftp.root = root.to_string_lossy().to_string();
            }
            if let Some(root) = http_root {
                config.http.root = root.to_string_lossy().to_string();
            }
            if let Some(interface) = interface {
                config.dhcp.interface = Some(interface);
            }
            if let Some(enabled) = enable_efi {
                config.dhcp.protocols.efi = enabled;
            }
            if let Some(enabled) = enable_legacy {
                config.dhcp.protocols.legacy = enabled;
            }
            if let Some(enabled) = enable_dhcp_boot {
                config.dhcp.protocols.dhcp_boot = enabled;
            }

            server::Server::new(config)?.start().await?;
        }
        None => {
            // Default behavior: start server with default config
            let config = config::Config::default();
            server::Server::new(config)?.start().await?;
        }
    }

    Ok(())
}
