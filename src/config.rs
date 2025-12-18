use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub dhcp: DhcpConfig,
    pub tftp: TftpConfig,
    pub http: HttpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub port: u16,
    pub interface: Option<String>,
    pub protocols: ProtocolConfig,
    pub ip_pool_start: String,
    pub ip_pool_end: String,
    pub subnet_mask: String,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
    pub next_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub efi: bool,
    pub legacy: bool,
    pub dhcp_boot: bool,
    #[serde(default)]
    pub boot_filename_efi: Option<String>,
    #[serde(default)]
    pub boot_filename_legacy: Option<String>,
    #[serde(default)]
    pub boot_filename_dhcp_boot: Option<String>,
    /// Force a specific protocol ("efi", "legacy", or "dhcp_boot") instead of auto-detection
    #[serde(default)]
    pub force_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TftpConfig {
    pub port: u16,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub port: u16,
    pub root: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dhcp: DhcpConfig {
                port: 67,
                interface: None,
                protocols: ProtocolConfig {
                    efi: true,
                    legacy: true,
                    dhcp_boot: true,
                    boot_filename_efi: None,
                    boot_filename_legacy: None,
                    boot_filename_dhcp_boot: None,
                    force_protocol: None,
                },
                ip_pool_start: "192.168.1.100".to_string(),
                ip_pool_end: "192.168.1.200".to_string(),
                subnet_mask: "255.255.255.0".to_string(),
                gateway: Some("192.168.1.1".to_string()),
                dns_servers: vec!["8.8.8.8".to_string()],
                next_server: "192.168.1.1".to_string(),
            },
            tftp: TftpConfig {
                port: 69,
                root: "./tftp".to_string(),
            },
            http: HttpConfig {
                port: 8080,
                root: "./http".to_string(),
            },
        }
    }
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.dhcp.port, 67);
        assert_eq!(config.tftp.port, 69);
        assert_eq!(config.http.port, 8080);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.dhcp.port, parsed.dhcp.port);
    }

    #[test]
    fn test_config_with_custom_boot_filenames() {
        let config_str = r#"
[dhcp]
port = 67
interface = ""
ip_pool_start = "192.168.1.100"
ip_pool_end = "192.168.1.200"
subnet_mask = "255.255.255.0"
gateway = "192.168.1.1"
dns_servers = ["8.8.8.8"]
next_server = "192.168.1.1"

[dhcp.protocols]
efi = true
legacy = true
dhcp_boot = true
boot_filename_efi = "custom_efi.efi"
boot_filename_legacy = "custom_legacy.0"
boot_filename_dhcp_boot = "custom_dhcp.0"

[tftp]
port = 69
root = "./tftp"

[http]
port = 8080
root = "./http"
"#;
        let config: Config = toml::from_str(config_str).unwrap();
        assert_eq!(
            config.dhcp.protocols.boot_filename_efi,
            Some("custom_efi.efi".to_string())
        );
        assert_eq!(
            config.dhcp.protocols.boot_filename_legacy,
            Some("custom_legacy.0".to_string())
        );
        assert_eq!(
            config.dhcp.protocols.boot_filename_dhcp_boot,
            Some("custom_dhcp.0".to_string())
        );
    }

    #[test]
    fn test_config_without_custom_boot_filenames() {
        let config_str = r#"
[dhcp]
port = 67
interface = ""
ip_pool_start = "192.168.1.100"
ip_pool_end = "192.168.1.200"
subnet_mask = "255.255.255.0"
gateway = "192.168.1.1"
dns_servers = ["8.8.8.8"]
next_server = "192.168.1.1"

[dhcp.protocols]
efi = true
legacy = true
dhcp_boot = true

[tftp]
port = 69
root = "./tftp"

[http]
port = 8080
root = "./http"
"#;
        let config: Config = toml::from_str(config_str).unwrap();
        assert_eq!(config.dhcp.protocols.boot_filename_efi, None);
        assert_eq!(config.dhcp.protocols.boot_filename_legacy, None);
        assert_eq!(config.dhcp.protocols.boot_filename_dhcp_boot, None);
    }
}
