use finiky::config::Config;
use finiky::dhcp::protocols::{BootProtocol, ProtocolHandler};
use finiky::dhcp::server::{DhcpMessage, DhcpServer};

#[test]
fn test_dhcp_message_serialization() {
    let msg = DhcpMessage {
        op: 1,
        htype: 1,
        hlen: 6,
        hops: 0,
        xid: 0x12345678,
        secs: 0,
        flags: 0,
        ciaddr: "0.0.0.0".parse().unwrap(),
        yiaddr: "0.0.0.0".parse().unwrap(),
        siaddr: "0.0.0.0".parse().unwrap(),
        giaddr: "0.0.0.0".parse().unwrap(),
        chaddr: [0u8; 16],
        sname: [0u8; 64],
        file: [0u8; 128],
        options: vec![53, 1, 1, 255], // DHCP Discover
    };

    let bytes = msg.to_bytes();
    let parsed = DhcpMessage::from_bytes(&bytes).unwrap();

    assert_eq!(msg.op, parsed.op);
    assert_eq!(msg.xid, parsed.xid);
}

#[test]
fn test_dhcp_message_options() {
    let msg = DhcpMessage {
        op: 1,
        htype: 1,
        hlen: 6,
        hops: 0,
        xid: 0,
        secs: 0,
        flags: 0,
        ciaddr: "0.0.0.0".parse().unwrap(),
        yiaddr: "0.0.0.0".parse().unwrap(),
        siaddr: "0.0.0.0".parse().unwrap(),
        giaddr: "0.0.0.0".parse().unwrap(),
        chaddr: [0u8; 16],
        sname: [0u8; 64],
        file: [0u8; 128],
        options: vec![53, 1, 1, 93, 2, 0, 6, 255], // Message type + Client arch (EFI)
    };

    assert_eq!(msg.get_message_type(), Some(1));
    assert_eq!(msg.get_client_arch(), Some(6));
}

#[test]
fn test_protocol_handler() {
    use finiky::config::ProtocolConfig;

    let config = ProtocolConfig {
        efi: true,
        legacy: true,
        dhcp_boot: true,
        boot_filename_efi: None,
        boot_filename_legacy: None,
        force_protocol: None,
    };

    // Test EFI architecture
    assert_eq!(
        ProtocolHandler::select_protocol(&config, Some(6)),
        Some(BootProtocol::Efi)
    );

    // Test Legacy architecture
    assert_eq!(
        ProtocolHandler::select_protocol(&config, Some(0)),
        Some(BootProtocol::Legacy)
    );

    // Test default (no arch specified)
    assert_eq!(
        ProtocolHandler::select_protocol(&config, None),
        Some(BootProtocol::Efi) // EFI is checked first
    );

    // Test disabled protocols
    let config_disabled = ProtocolConfig {
        efi: false,
        legacy: true,
        dhcp_boot: false,
        boot_filename_efi: None,
        boot_filename_legacy: None,
        force_protocol: None,
    };
    assert_eq!(
        ProtocolHandler::select_protocol(&config_disabled, None),
        Some(BootProtocol::Legacy)
    );
}

#[test]
fn test_boot_filename() {
    use finiky::config::ProtocolConfig;

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
    // DhcpBoot mode now uses the same filenames as EFI/Legacy modes
    assert_eq!(
        ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(0)),
        "pxelinux.0"
    );
    assert_eq!(
        ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(6)),
        "bootx64.efi"
    );
}

#[test]
fn test_boot_filename_custom() {
    use finiky::config::ProtocolConfig;

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
        ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(0)),
        "custom_legacy.0"
    );
    assert_eq!(
        ProtocolHandler::get_boot_filename(BootProtocol::DhcpBoot, &config, Some(6)),
        "custom_efi.efi"
    );
}

#[test]
fn test_dhcp_server_creation() {
    let config = Config::default();
    let server = DhcpServer::new(config.dhcp);
    assert!(server.is_ok());
}
