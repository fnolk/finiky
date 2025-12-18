use crate::config::ProtocolConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BootProtocol {
    Efi,
    Legacy,
    DhcpBoot,
}

pub struct ProtocolHandler;

impl ProtocolHandler {
    pub fn select_protocol(
        config: &ProtocolConfig,
        client_arch: Option<u16>,
    ) -> Option<BootProtocol> {
        // Check if protocol is forced in config
        if let Some(ref forced) = config.force_protocol {
            return match forced.to_lowercase().as_str() {
                "efi" if config.efi => Some(BootProtocol::Efi),
                "legacy" if config.legacy => Some(BootProtocol::Legacy),
                "dhcp_boot" if config.dhcp_boot => Some(BootProtocol::DhcpBoot),
                _ => {
                    // If forced protocol is not valid or not enabled, fall back to auto-detection
                    tracing::warn!(
                        "Invalid or disabled forced protocol '{}', falling back to auto-detection",
                        forced
                    );
                    None
                }
            };
        }

        // Check client architecture option (option 93)
        if let Some(arch) = client_arch {
            match arch {
                6 => {
                    return if config.efi {
                        Some(BootProtocol::Efi)
                    } else {
                        None
                    }
                }
                0 | 1 => {
                    return if config.legacy {
                        Some(BootProtocol::Legacy)
                    } else {
                        None
                    }
                }
                _ => {}
            }
        }

        // Default selection based on enabled protocols
        if config.efi {
            Some(BootProtocol::Efi)
        } else if config.legacy {
            Some(BootProtocol::Legacy)
        } else if config.dhcp_boot {
            Some(BootProtocol::DhcpBoot)
        } else {
            None
        }
    }

    pub fn get_boot_filename(
        protocol: BootProtocol,
        config: &ProtocolConfig,
        client_arch: Option<u16>,
    ) -> String {
        match protocol {
            BootProtocol::Efi => config
                .boot_filename_efi
                .clone()
                .unwrap_or_else(|| "bootx64.efi".to_string()),
            BootProtocol::Legacy => config
                .boot_filename_legacy
                .clone()
                .unwrap_or_else(|| "pxelinux.0".to_string()),
            BootProtocol::DhcpBoot => {
                // Determine if client is EFI or Legacy based on architecture option
                let is_efi = if let Some(arch) = client_arch {
                    matches!(arch, 6 | 7 | 9) // EFI x86_64, EFI BC, EFI x64
                } else {
                    false // Unknown architecture defaults to Legacy
                };

                if is_efi {
                    config
                        .boot_filename_efi
                        .clone()
                        .unwrap_or_else(|| "bootx64.efi".to_string())
                } else {
                    config
                        .boot_filename_legacy
                        .clone()
                        .unwrap_or_else(|| "pxelinux.0".to_string())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProtocolConfig;

    #[test]
    fn test_protocol_selection() {
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: None,
        };

        assert_eq!(
            ProtocolHandler::select_protocol(&config, Some(6)),
            Some(BootProtocol::Efi)
        );
        assert_eq!(
            ProtocolHandler::select_protocol(&config, Some(0)),
            Some(BootProtocol::Legacy)
        );
    }

    #[test]
    fn test_boot_filename() {
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: None,
        };

        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::Efi, &config, None),
            "bootx64.efi"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::Legacy, &config, None),
            "pxelinux.0"
        );
    }

    #[test]
    fn test_boot_filename_custom() {
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: Some("custom_efi.efi".to_string()),
            boot_filename_legacy: Some("custom_legacy.0".to_string()),
            force_protocol: None,
        };

        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::Efi, &config, None),
            "custom_efi.efi"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::Legacy, &config, None),
            "custom_legacy.0"
        );
        // DhcpBoot mode now uses the same filenames as EFI/Legacy modes
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(6)),
            "custom_efi.efi"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(0)),
            "custom_legacy.0"
        );
    }

    #[test]
    fn test_forced_protocol() {
        // Test forcing EFI protocol
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: Some("efi".to_string()),
        };
        // Should return EFI even if client requests Legacy (arch 0)
        assert_eq!(
            ProtocolHandler::select_protocol(&config, Some(0)),
            Some(BootProtocol::Efi)
        );

        // Test forcing Legacy protocol
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: Some("legacy".to_string()),
        };
        // Should return Legacy even if client requests EFI (arch 6)
        assert_eq!(
            ProtocolHandler::select_protocol(&config, Some(6)),
            Some(BootProtocol::Legacy)
        );

        // Test forcing DhcpBoot protocol
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: Some("dhcp_boot".to_string()),
        };
        assert_eq!(
            ProtocolHandler::select_protocol(&config, Some(6)),
            Some(BootProtocol::DhcpBoot)
        );

        // Test forcing disabled protocol (should return None as fallback fails)
        let config = ProtocolConfig {
            efi: false,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: Some("efi".to_string()),
        };
        assert_eq!(ProtocolHandler::select_protocol(&config, Some(0)), None);
    }

    #[test]
    fn test_dhcp_boot_dual_architecture() {
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: None,
            boot_filename_legacy: None,
            force_protocol: None,
        };

        // Test EFI architectures (6, 7, 9) - now uses boot_filename_efi default
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(6)),
            "bootx64.efi"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(7)),
            "bootx64.efi"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(9)),
            "bootx64.efi"
        );

        // Test Legacy architectures (0, 1) - still uses boot_filename_legacy default
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(0)),
            "pxelinux.0"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(1)),
            "pxelinux.0"
        );

        // Test unknown architecture (defaults to Legacy)
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, None),
            "pxelinux.0"
        );
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(99)),
            "pxelinux.0"
        );
    }

    #[test]
    fn test_dhcp_boot_custom_filenames() {
        // Now DhcpBoot mode uses boot_filename_efi and boot_filename_legacy
        let config = ProtocolConfig {
            efi: true,
            legacy: true,
            dhcp_boot: true,
            boot_filename_efi: Some("custom_bootx64.efi".to_string()),
            boot_filename_legacy: Some("custom_pxelinux.0".to_string()),
            force_protocol: None,
        };

        // Test custom filenames for EFI
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(6)),
            "custom_bootx64.efi"
        );

        // Test custom filenames for Legacy
        assert_eq!(
            ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(0)),
            "custom_pxelinux.0"
        );
    }
}
