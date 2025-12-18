# Finiky - PXE Server

A fast and portable PXE Server written in Rust for rapid OS deployment to bare metal machines.

## Features

- **Easy CLI**: Start DHCP + TFTP + HTTP servers with simple command-line interface
- **Multi-protocol DHCP**: Supports EFI, Legacy BIOS, and DHCP-boot protocols
- **Flexible File Serving**: Serve files from directories or directly from tar.gz archives (virtual filesystem)
- **Portable**: Built with musl support, avoiding GNU-specific dependencies
- **Configurable**: Support for configuration files with command-line overrides

## Installation

```bash
cargo build --release
```

To build for musl-based static binary (for Linux deployment):

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

You can also set an alias for convenience:

```bash
# Add to your shell config (~/.zshrc or ~/.bashrc)
alias cargo-musl='cargo build --release --target x86_64-unknown-linux-musl'

# Then use it
cargo-musl
```

## Usage

### Generate Default Configuration

```bash
# Generate config.toml (default filename)
finiky gen-config

# Generate with custom filename
finiky gen-config my-config.toml
```

This will generate the default configuration file that you can customize.

### Start Server

```bash
# Using default configuration
finiky start

# With configuration file
finiky start --config /path/to/config.toml

# With command-line overrides
finiky start --dhcp-port 67 --tftp-port 69 --http-port 8080 \
  --tftp-root ./tftp --http-root ./http

# Enable/disable specific protocols
finiky start --enable-efi true --enable-legacy false
```

### Configuration File

The configuration file supports the following options:

```toml
[dhcp]
port = 67
interface = "null"  # Optional network interface name
ip_pool_start = "192.168.1.100"
ip_pool_end = "192.168.1.200"
subnet_mask = "255.255.255.0"
gateway = "192.168.1.1"
dns_servers = ["8.8.8.8", "8.8.4.4"]
next_server = "192.168.1.1"

[dhcp.protocols]
efi = true
legacy = true
dhcp_boot = true

[tftp]
port = 69
root = "./tftp"  # Directory or tar.gz file

[http]
port = 8080
root = "./http"  # Directory or tar.gz file
```

## Features in Detail

### Virtual Filesystem

The server can serve files from:
- **Directories**: Standard directory-based file serving
- **tar.gz archives**: Files are read directly from compressed archives without extraction

Example:
```bash
# Serve from directory
finiky start --tftp-root ./boot_files

# Serve from tar.gz archive
finiky start --tftp-root ./boot_files.tar.gz
```

### DHCP Protocols

- **EFI**: Returns `bootx64.efi` for UEFI systems
- **Legacy**: Returns `pxelinux.0` for legacy BIOS systems
- **DHCP-boot**: Standard DHCP boot protocol

The server automatically detects the client architecture and responds accordingly.

## Testing

### Unit and Integration Tests

Run all tests:

```bash
cargo test
```

Run specific test suite:

```bash
cargo test --test filesystem_tests
cargo test --test dhcp_tests
cargo test --test tftp_tests
cargo test --test http_tests
cargo test --test integration_tests
```

### QEMU PXE End-to-End Testing

For comprehensive validation of the PXE server functionality, you can use QEMU to simulate a complete PXE boot process. This tests the full workflow including DHCP IP allocation, TFTP file transfer, and HTTP file serving.

#### Prerequisites

- **QEMU**: `qemu-system-x86_64` (for virtualization)
- **syslinux**: For PXE boot files (`pxelinux.0`, `bootx64.efi`)
- **iproute2**: For network configuration (`ip` command)
- **Root privileges**: Required for TAP device creation

Install dependencies:

```bash
# Ubuntu/Debian
sudo apt-get install qemu-system-x86 qemu-utils syslinux-common iproute2

# CentOS/RHEL
sudo yum install qemu-system-x86 syslinux iproute
```

#### Running QEMU PXE Tests

The test script automates the entire PXE boot process:

1. **Prepare PXE files**: Automatically prepares boot files from syslinux
2. **Setup network**: Creates isolated TAP network interface
3. **Start finiky server**: Launches the PXE server with test configuration
4. **Launch QEMU**: Boots a virtual machine via PXE
5. **Verify boot process**: Checks DHCP, TFTP, and HTTP functionality

Run the test:

```bash
# Test both UEFI and Legacy BIOS modes (default)
sudo tests/qemu_pxe_test.sh

# Test only UEFI mode
sudo tests/qemu_pxe_test.sh uefi

# Test only Legacy BIOS mode
sudo tests/qemu_pxe_test.sh legacy
```

#### Test Configuration

You can customize the test environment using environment variables:

```bash
# Customize TAP device and network
export TAP_DEVICE=tap1
export TAP_IP=192.168.200.1
export TAP_NETMASK=255.255.255.0

# Adjust QEMU memory and timeout
export QEMU_MEMORY=1G
export TEST_TIMEOUT=180

# Run test
sudo tests/qemu_pxe_test.sh
```

#### Test Process

The test script performs the following steps:

1. **Environment Setup**:
   - Builds finiky server in release mode
   - Prepares PXE boot files (pxelinux.0, bootx64.efi)
   - Creates TAP network interface (tap0)
   - Configures network with IP 192.168.100.1/24

2. **Server Startup**:
   - Starts finiky with test configuration
   - DHCP server on port 67
   - TFTP server on port 69
   - HTTP server on port 8080

3. **QEMU Boot**:
   - Launches QEMU with PXE boot enabled
   - Virtual machine requests IP via DHCP
   - Downloads boot file via TFTP
   - Displays boot menu

4. **Verification**:
   - Checks serial output for PXE activity
   - Verifies DHCP IP allocation
   - Confirms TFTP file transfer
   - Validates boot process completion

#### Test Output

Test logs are saved in the `tests/` directory:

- `finiky.log`: finiky server logs
- `qemu_uefi.log`: QEMU UEFI boot logs
- `qemu_uefi_serial.log`: Serial output from UEFI boot
- `qemu_legacy.log`: QEMU Legacy BIOS boot logs
- `qemu_legacy_serial.log`: Serial output from Legacy BIOS boot

#### Troubleshooting

**TAP device creation fails**:
- Ensure you have root privileges
- Check if `ip tuntap` command is available
- Verify kernel supports TAP devices

**QEMU cannot find OVMF firmware**:
- Install OVMF package: `sudo apt-get install ovmf` (Ubuntu/Debian)
- Or download from: https://github.com/tianocore/edk2
- UEFI tests will be skipped if firmware is not found

**PXE boot files missing**:
- Install syslinux: `sudo apt-get install syslinux-common`
- Or manually download from: https://mirrors.edge.kernel.org/pub/linux/utils/boot/syslinux/
- The script will provide download instructions if files are missing

**Network conflicts**:
- Change TAP_IP and TAP_DEVICE if conflicts occur
- Ensure no other service uses ports 67, 69, or 8080

**QEMU hangs or times out**:
- Increase TEST_TIMEOUT environment variable
- Check QEMU logs for errors
- Verify network connectivity between QEMU and finiky server

## Architecture

- **DHCP Server**: Handles PXE boot requests, IP allocation, and protocol selection
- **TFTP Server**: Serves boot files using TFTP protocol (RFC 1350)
- **HTTP Server**: Serves larger files and installation media via HTTP
- **Virtual Filesystem**: Abstract filesystem layer supporting directories and tar.gz archives

## License

MIT

