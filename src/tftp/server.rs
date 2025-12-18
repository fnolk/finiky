use crate::filesystem::FileSystem;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing as log;

const BLOCK_SIZE: usize = 512;
const MAX_PACKET_SIZE: usize = 516; // 4 bytes header + 512 bytes data

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TftpOpcode {
    ReadRequest = 1,
    WriteRequest = 2,
    Data = 3,
    Ack = 4,
    Error = 5,
}

#[derive(Debug)]
pub struct TftpPacket {
    opcode: TftpOpcode,
    data: Vec<u8>,
}

impl TftpPacket {
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 2 {
            return Err("Packet too short".to_string());
        }

        let opcode = u16::from_be_bytes([data[0], data[1]]);
        let opcode = match opcode {
            1 => TftpOpcode::ReadRequest,
            2 => TftpOpcode::WriteRequest,
            3 => TftpOpcode::Data,
            4 => TftpOpcode::Ack,
            5 => TftpOpcode::Error,
            _ => return Err(format!("Unknown opcode: {}", opcode)),
        };

        Ok(TftpPacket {
            opcode,
            data: data[2..].to_vec(),
        })
    }

    #[allow(dead_code)]
    pub fn build_ack(block_num: u16) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&(TftpOpcode::Ack as u16).to_be_bytes());
        packet.extend_from_slice(&block_num.to_be_bytes());
        packet
    }

    pub fn build_data(block_num: u16, data: &[u8]) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&(TftpOpcode::Data as u16).to_be_bytes());
        packet.extend_from_slice(&block_num.to_be_bytes());
        packet.extend_from_slice(data);
        packet
    }

    pub fn build_error(code: u16, message: &str) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&(TftpOpcode::Error as u16).to_be_bytes());
        packet.extend_from_slice(&code.to_be_bytes());
        packet.extend_from_slice(message.as_bytes());
        packet.push(0); // Null terminator
        packet
    }

    pub fn build_oack(options: &HashMap<String, String>) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&6u16.to_be_bytes()); // OACK opcode = 6
        for (key, value) in options {
            packet.extend_from_slice(key.as_bytes());
            packet.push(0);
            packet.extend_from_slice(value.as_bytes());
            packet.push(0);
        }
        packet
    }

    pub fn extract_filename(&self) -> Option<String> {
        if matches!(
            self.opcode,
            TftpOpcode::ReadRequest | TftpOpcode::WriteRequest
        ) {
            let null_pos = self.data.iter().position(|&b| b == 0)?;
            String::from_utf8(self.data[..null_pos].to_vec()).ok()
        } else {
            None
        }
    }

    pub fn extract_options(&self) -> HashMap<String, String> {
        let mut options = HashMap::new();
        if !matches!(
            self.opcode,
            TftpOpcode::ReadRequest | TftpOpcode::WriteRequest
        ) {
            return options;
        }

        // Skip filename and mode
        let mut pos = 0;
        for _ in 0..2 {
            if let Some(null_pos) = self.data[pos..].iter().position(|&b| b == 0) {
                pos += null_pos + 1;
            } else {
                return options;
            }
        }

        // Parse options (key-value pairs separated by null bytes)
        while pos < self.data.len() {
            if let Some(null_pos) = self.data[pos..].iter().position(|&b| b == 0) {
                if let Ok(key) = String::from_utf8(self.data[pos..pos + null_pos].to_vec()) {
                    pos += null_pos + 1;
                    if pos < self.data.len() {
                        if let Some(null_pos2) = self.data[pos..].iter().position(|&b| b == 0) {
                            if let Ok(value) =
                                String::from_utf8(self.data[pos..pos + null_pos2].to_vec())
                            {
                                options.insert(key.to_lowercase(), value);
                            }
                            pos += null_pos2 + 1;
                        } else {
                            break;
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        options
    }

    #[allow(dead_code)]
    pub fn opcode(&self) -> TftpOpcode {
        self.opcode
    }
}

pub struct TftpServer {
    port: u16,
    filesystem: Arc<dyn FileSystem>,
}

impl TftpServer {
    pub fn new(port: u16, filesystem: Box<dyn FileSystem>) -> Self {
        TftpServer {
            port,
            filesystem: Arc::from(filesystem),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let socket = Arc::new(UdpSocket::bind(format!("0.0.0.0:{}", self.port)).await?);
        log::info!("TFTP server listening on port {}", self.port);

        let mut buf = vec![0u8; MAX_PACKET_SIZE];
        let filesystem = Arc::clone(&self.filesystem);

        // Track active transfers: peer -> sender channel
        let active_transfers: Arc<tokio::sync::Mutex<HashMap<SocketAddr, mpsc::Sender<Vec<u8>>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((size, peer)) => {
                    let data = &buf[..size];
                    if let Ok(packet) = TftpPacket::parse(data) {
                        match packet.opcode {
                            TftpOpcode::ReadRequest => {
                                let socket_clone = Arc::clone(&socket);
                                let filesystem_clone = Arc::clone(&filesystem);
                                let active_transfers_clone = Arc::clone(&active_transfers);

                                // Create a channel for this transfer
                                let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
                                active_transfers_clone.lock().await.insert(peer, tx);

                                if let Some(filename) = packet.extract_filename() {
                                    let options = packet.extract_options();
                                    log::info!("TFTP read request for: {} from {}", filename, peer);
                                    tokio::spawn(Self::handle_read_with_channel(
                                        socket_clone,
                                        peer,
                                        filename,
                                        options,
                                        filesystem_clone,
                                        active_transfers_clone,
                                        rx,
                                    ));
                                }
                            }
                            TftpOpcode::Ack => {
                                // Route ACK to the appropriate transfer handler
                                let active_transfers_clone = Arc::clone(&active_transfers);
                                let tx_opt = {
                                    let transfers = active_transfers_clone.lock().await;
                                    transfers.get(&peer).cloned()
                                };

                                if let Some(tx) = tx_opt {
                                    if tx.send(data.to_vec()).await.is_err() {
                                        log::warn!(
                                            "Failed to send ACK to transfer handler for {}",
                                            peer
                                        );
                                        active_transfers_clone.lock().await.remove(&peer);
                                    }
                                } else {
                                    log::warn!("Received ACK from {} but no active transfer", peer);
                                }
                            }
                            TftpOpcode::WriteRequest => {
                                let error = TftpPacket::build_error(2, "Write not supported");
                                let _ = socket.send_to(&error, peer).await;
                            }
                            _ => {
                                log::warn!(
                                    "Unexpected TFTP packet type {:?} from {}",
                                    packet.opcode,
                                    peer
                                );
                            }
                        }
                    } else {
                        log::warn!("Failed to parse TFTP packet from {}", peer);
                    }
                }
                Err(e) => {
                    log::error!("TFTP receive error: {}", e);
                }
            }
        }
    }

    async fn handle_read_with_channel(
        socket: Arc<UdpSocket>,
        peer: SocketAddr,
        filename: String,
        options: HashMap<String, String>,
        filesystem: Arc<dyn FileSystem>,
        active_transfers: Arc<tokio::sync::Mutex<HashMap<SocketAddr, mpsc::Sender<Vec<u8>>>>>,
        mut ack_rx: mpsc::Receiver<Vec<u8>>,
    ) {
        // Normalize filename (remove leading slash if present)
        let filename = filename.trim_start_matches('/');

        if !filesystem.exists(filename).await {
            log::warn!("TFTP file not found: {}", filename);
            let error = TftpPacket::build_error(1, "File not found");
            let _ = socket.send_to(&error, peer).await;
            return;
        }

        let file_data = match filesystem.read_file(filename).await {
            Ok(data) => data,
            Err(e) => {
                log::error!("Error reading file {}: {}", filename, e);
                let error = TftpPacket::build_error(0, "Error reading file");
                let _ = socket.send_to(&error, peer).await;
                return;
            }
        };

        // Handle options if present (RFC 2347)
        let mut block_size = BLOCK_SIZE;
        if !options.is_empty() {
            let mut ack_options = HashMap::new();

            // Handle blksize option
            if let Some(blksize_str) = options.get("blksize") {
                if let Ok(requested_size) = blksize_str.parse::<usize>() {
                    // RFC 2348: blksize range is 8-65464 bytes
                    if (8..=65464).contains(&requested_size) {
                        block_size = requested_size;
                        ack_options.insert("blksize".to_string(), blksize_str.clone());
                    }
                }
            }

            // Handle tsize option (RFC 2349)
            if options.contains_key("tsize") {
                ack_options.insert("tsize".to_string(), file_data.len().to_string());
            }

            // Send OACK
            let oack = TftpPacket::build_oack(&ack_options);
            if let Err(e) = socket.send_to(&oack, peer).await {
                log::error!("Error sending OACK: {}", e);
                return;
            }

            // Wait for ACK of block 0 (OACK acknowledgment)
            match tokio::time::timeout(std::time::Duration::from_secs(5), ack_rx.recv()).await {
                Ok(Some(ack_data)) => {
                    if ack_data.len() >= 4 {
                        let ack_opcode = u16::from_be_bytes([ack_data[0], ack_data[1]]);
                        let ack_block = u16::from_be_bytes([ack_data[2], ack_data[3]]);

                        if ack_opcode != 4 || ack_block != 0 {
                            log::warn!(
                                "Invalid ACK for OACK from {}: opcode={}, block={}",
                                peer,
                                ack_opcode,
                                ack_block
                            );
                            active_transfers.lock().await.remove(&peer);
                            return;
                        }
                    } else {
                        log::warn!("ACK packet too short from {}", peer);
                        active_transfers.lock().await.remove(&peer);
                        return;
                    }
                }
                Ok(None) => {
                    log::warn!("ACK channel closed for {}", peer);
                    active_transfers.lock().await.remove(&peer);
                    return;
                }
                Err(_) => {
                    log::warn!("Timeout waiting for OACK ACK from {}", peer);
                    active_transfers.lock().await.remove(&peer);
                    return;
                }
            }
        }

        // Send file in blocks
        let mut block_num = 1u16;
        let mut offset = 0;

        loop {
            let remaining = file_data.len() - offset;
            let chunk_size = remaining.min(block_size);
            let chunk = &file_data[offset..offset + chunk_size];

            let data_packet = TftpPacket::build_data(block_num, chunk);

            // Send data packet
            if let Err(e) = socket.send_to(&data_packet, peer).await {
                log::error!("Error sending TFTP data: {}", e);
                return;
            }

            // Wait for ACK via channel
            match tokio::time::timeout(std::time::Duration::from_secs(5), ack_rx.recv()).await {
                Ok(Some(ack_data)) => {
                    if ack_data.len() >= 4 {
                        let ack_opcode = u16::from_be_bytes([ack_data[0], ack_data[1]]);
                        let ack_block = u16::from_be_bytes([ack_data[2], ack_data[3]]);

                        if ack_opcode == TftpOpcode::Ack as u16 && ack_block == block_num {
                            offset += chunk_size;
                            log::debug!("Received ACK for block {} of {}", block_num, filename);

                            // If this was the last block (less than block_size), we're done
                            if chunk_size < block_size {
                                log::info!(
                                    "TFTP transfer complete: {} ({} bytes)",
                                    filename,
                                    file_data.len()
                                );
                                active_transfers.lock().await.remove(&peer);
                                return;
                            }

                            block_num = block_num.wrapping_add(1);
                            if block_num == 0 {
                                block_num = 1; // Wrap around (though unlikely)
                            }
                        } else {
                            log::warn!(
                                "Invalid ACK from {}: expected block {}, got {}",
                                peer,
                                block_num,
                                ack_block
                            );
                            active_transfers.lock().await.remove(&peer);
                            return;
                        }
                    } else {
                        log::warn!("ACK packet too short from {}", peer);
                        active_transfers.lock().await.remove(&peer);
                        return;
                    }
                }
                Ok(None) => {
                    log::warn!("ACK channel closed for {}", peer);
                    active_transfers.lock().await.remove(&peer);
                    return;
                }
                Err(_) => {
                    log::warn!("Timeout waiting for ACK from {}", peer);
                    active_transfers.lock().await.remove(&peer);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tftp_packet_parsing() {
        let mut data = Vec::new();
        data.extend_from_slice(&(TftpOpcode::ReadRequest as u16).to_be_bytes());
        data.extend_from_slice(b"test.txt");
        data.push(0);
        data.extend_from_slice(b"octet");
        data.push(0);

        let packet = TftpPacket::parse(&data).unwrap();
        assert!(matches!(packet.opcode(), TftpOpcode::ReadRequest));
        assert_eq!(packet.extract_filename(), Some("test.txt".to_string()));
    }

    #[tokio::test]
    async fn test_tftp_ack() {
        let ack = TftpPacket::build_ack(1);
        assert_eq!(ack.len(), 4);
        assert_eq!(u16::from_be_bytes([ack[0], ack[1]]), TftpOpcode::Ack as u16);
        assert_eq!(u16::from_be_bytes([ack[2], ack[3]]), 1);
    }
}
