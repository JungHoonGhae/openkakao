use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{anyhow, Result};
use bson::Document;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::Cursor;

pub const HEADER_SIZE: usize = 22;
/// Maximum allowed packet body size (100 MB) to prevent memory exhaustion from untrusted input.
const MAX_BODY_SIZE: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LocoPacket {
    pub packet_id: u32,
    pub status_code: i16,
    pub method: String,
    pub body_type: u8,
    pub body: Document,
}

impl LocoPacket {
    pub fn encode(&self) -> Vec<u8> {
        let body_bytes = bson::to_vec(&self.body).unwrap_or_default();

        let mut buf = Vec::with_capacity(HEADER_SIZE + body_bytes.len());

        buf.write_u32::<LittleEndian>(self.packet_id).unwrap();
        buf.write_i16::<LittleEndian>(self.status_code).unwrap();

        // Method: 11 bytes ASCII null-padded
        let method_bytes = self.method.as_bytes();
        let mut method_buf = [0u8; 11];
        let copy_len = method_bytes.len().min(11);
        method_buf[..copy_len].copy_from_slice(&method_bytes[..copy_len]);
        buf.extend_from_slice(&method_buf);

        buf.write_u8(self.body_type).unwrap();
        buf.write_u32::<LittleEndian>(body_bytes.len() as u32)
            .unwrap();

        buf.extend_from_slice(&body_bytes);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(anyhow!("Data too short: {} < {}", data.len(), HEADER_SIZE));
        }

        let mut cursor = Cursor::new(data);
        let packet_id = cursor.read_u32::<LittleEndian>()?;
        let status_code = cursor.read_i16::<LittleEndian>()?;

        let method = {
            let method_bytes = &data[6..17];
            let end = method_bytes.iter().position(|&b| b == 0).unwrap_or(11);
            String::from_utf8_lossy(&method_bytes[..end]).to_string()
        };

        let body_type = data[17];

        let mut cursor = Cursor::new(&data[18..]);
        let body_length = cursor.read_u32::<LittleEndian>()? as usize;

        if body_length > MAX_BODY_SIZE {
            return Err(anyhow!(
                "Packet body too large: {} > {}",
                body_length,
                MAX_BODY_SIZE
            ));
        }
        if HEADER_SIZE + body_length > data.len() {
            return Err(anyhow!(
                "Packet truncated: need {} bytes, have {}",
                HEADER_SIZE + body_length,
                data.len()
            ));
        }

        let body_data = &data[HEADER_SIZE..HEADER_SIZE + body_length];
        let body = if body_data.is_empty() {
            Document::new()
        } else {
            bson::from_slice(body_data)?
        };

        Ok(Self {
            packet_id,
            status_code,
            method,
            body_type,
            body,
        })
    }

    pub fn decode_header(data: &[u8]) -> Result<(u32, i16, String, u8, u32)> {
        if data.len() < HEADER_SIZE {
            return Err(anyhow!(
                "Header too short: {} < {}",
                data.len(),
                HEADER_SIZE
            ));
        }

        let mut cursor = Cursor::new(data);
        let packet_id = cursor.read_u32::<LittleEndian>()?;
        let status_code = cursor.read_i16::<LittleEndian>()?;

        let method = {
            let method_bytes = &data[6..17];
            let end = method_bytes.iter().position(|&b| b == 0).unwrap_or(11);
            String::from_utf8_lossy(&method_bytes[..end]).to_string()
        };

        let body_type = data[17];

        let mut cursor = Cursor::new(&data[18..]);
        let body_length = cursor.read_u32::<LittleEndian>()?;

        Ok((packet_id, status_code, method, body_type, body_length))
    }

    pub fn status(&self) -> i64 {
        self.body
            .get_i64("status")
            .or_else(|_| self.body.get_i32("status").map(|v| v as i64))
            .unwrap_or(self.status_code as i64)
    }
}

pub struct PacketBuilder {
    next_id: AtomicU32,
}

impl Default for PacketBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketBuilder {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
        }
    }

    pub fn build(&self, method: &str, body: Document) -> LocoPacket {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        LocoPacket {
            packet_id: id,
            status_code: 0,
            method: method.to_string(),
            body_type: 0,
            body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut body = Document::new();
        body.insert("os", "mac");
        body.insert("userId", 12345_i64);

        let pkt = LocoPacket {
            packet_id: 1,
            status_code: 0,
            method: "GETCONF".to_string(),
            body_type: 0,
            body,
        };

        let encoded = pkt.encode();
        assert!(encoded.len() > HEADER_SIZE);

        let decoded = LocoPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.packet_id, 1);
        assert_eq!(decoded.status_code, 0);
        assert_eq!(decoded.method, "GETCONF");
        assert_eq!(decoded.body_type, 0);
        assert_eq!(decoded.body.get_str("os").unwrap(), "mac");
        assert_eq!(decoded.body.get_i64("userId").unwrap(), 12345);
    }

    #[test]
    fn test_method_truncation() {
        let pkt = LocoPacket {
            packet_id: 1,
            status_code: 0,
            method: "LONGERMETHOD".to_string(), // 12 chars, > 11
            body_type: 0,
            body: Document::new(),
        };

        let encoded = pkt.encode();
        let decoded = LocoPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.method, "LONGERMETHO"); // truncated to 11
    }

    #[test]
    fn test_decode_header() {
        let mut body = Document::new();
        body.insert("test", true);

        let pkt = LocoPacket {
            packet_id: 42,
            status_code: 0,
            method: "CHECKIN".to_string(),
            body_type: 0,
            body,
        };

        let encoded = pkt.encode();
        let (id, status, method, body_type, body_len) =
            LocoPacket::decode_header(&encoded).unwrap();
        assert_eq!(id, 42);
        assert_eq!(status, 0);
        assert_eq!(method, "CHECKIN");
        assert_eq!(body_type, 0);
        assert!(body_len > 0);
    }

    #[test]
    fn test_packet_builder_auto_increment() {
        let builder = PacketBuilder::new();
        let p1 = builder.build("A", Document::new());
        let p2 = builder.build("B", Document::new());
        let p3 = builder.build("C", Document::new());
        assert_eq!(p1.packet_id, 1);
        assert_eq!(p2.packet_id, 2);
        assert_eq!(p3.packet_id, 3);
    }

    #[test]
    fn test_decode_too_short() {
        let data = vec![0u8; 10];
        assert!(LocoPacket::decode(&data).is_err());
    }
}
