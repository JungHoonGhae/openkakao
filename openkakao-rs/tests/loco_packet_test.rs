use openkakao_rs::loco::packet::{LocoPacket, PacketBuilder, HEADER_SIZE};

#[test]
fn encode_decode_roundtrip_getconf() {
    let mut body = bson::Document::new();
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
    assert_eq!(decoded.body.get_str("os").unwrap(), "mac");
    assert_eq!(decoded.body.get_i64("userId").unwrap(), 12345);
}

#[test]
fn encode_decode_roundtrip_checkin() {
    let body = bson::doc! {
        "userId": 999_i64,
        "os": "mac",
        "netType": 0_i32,
        "appVer": "4.7.2",
    };

    let pkt = LocoPacket {
        packet_id: 42,
        status_code: 0,
        method: "CHECKIN".to_string(),
        body_type: 0,
        body,
    };

    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "CHECKIN");
    assert_eq!(decoded.body.get_i64("userId").unwrap(), 999);
    assert_eq!(decoded.body.get_str("appVer").unwrap(), "4.7.2");
}

#[test]
fn encode_decode_roundtrip_msg() {
    let body = bson::doc! {
        "chatId": 382416827148557_i64,
        "msg": "Hello, world!",
        "type": 1_i32,
        "logId": 9876543210_i64,
        "authorId": 405979308_i64,
    };

    let pkt = LocoPacket {
        packet_id: 7,
        status_code: 0,
        method: "MSG".to_string(),
        body_type: 0,
        body,
    };

    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "MSG");
    assert_eq!(decoded.body.get_i64("chatId").unwrap(), 382416827148557);
    assert_eq!(decoded.body.get_str("msg").unwrap(), "Hello, world!");
}

#[test]
fn empty_body_roundtrip() {
    let pkt = LocoPacket {
        packet_id: 10,
        status_code: 0,
        method: "PING".to_string(),
        body_type: 0,
        body: bson::Document::new(),
    };

    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "PING");
    assert!(decoded.body.is_empty());
}

#[test]
fn malformed_header_too_short() {
    let data = vec![0u8; 10];
    assert!(LocoPacket::decode(&data).is_err());
}

#[test]
fn malformed_header_exact_boundary() {
    let data = vec![0u8; HEADER_SIZE - 1];
    assert!(LocoPacket::decode(&data).is_err());
}

#[test]
fn truncated_body_rejected() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: 0,
        method: "TEST".to_string(),
        body_type: 0,
        body: bson::doc! { "key": "value" },
    };
    let mut encoded = pkt.encode();
    // Truncate the body
    encoded.truncate(HEADER_SIZE + 2);
    assert!(LocoPacket::decode(&encoded).is_err());
}

#[test]
fn status_from_body_field() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: 0,
        method: "LOGINLIST".to_string(),
        body_type: 0,
        body: bson::doc! { "status": -950_i64 },
    };
    assert_eq!(pkt.status(), -950);
}

#[test]
fn status_from_header_when_body_missing() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: -1,
        method: "FAIL".to_string(),
        body_type: 0,
        body: bson::Document::new(),
    };
    assert_eq!(pkt.status(), -1);
}

#[test]
fn status_from_body_i32() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: 0,
        method: "TEST".to_string(),
        body_type: 0,
        body: bson::doc! { "status": 0_i32 },
    };
    assert_eq!(pkt.status(), 0);
}

#[test]
fn packet_builder_auto_increments() {
    let builder = PacketBuilder::new();
    let p1 = builder.build("A", bson::Document::new());
    let p2 = builder.build("B", bson::Document::new());
    let p3 = builder.build("C", bson::Document::new());
    assert_eq!(p1.packet_id, 1);
    assert_eq!(p2.packet_id, 2);
    assert_eq!(p3.packet_id, 3);
}

#[test]
fn method_truncated_to_11_bytes() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: 0,
        method: "LONGERMETHOD".to_string(), // 12 chars
        body_type: 0,
        body: bson::Document::new(),
    };

    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "LONGERMETHO"); // truncated to 11
}

#[test]
fn decode_header_matches_full_decode() {
    let body = bson::doc! { "test": true };
    let pkt = LocoPacket {
        packet_id: 42,
        status_code: 0,
        method: "CHECKIN".to_string(),
        body_type: 0,
        body,
    };

    let encoded = pkt.encode();
    let (id, status, method, body_type, body_len) = LocoPacket::decode_header(&encoded).unwrap();
    assert_eq!(id, 42);
    assert_eq!(status, 0);
    assert_eq!(method, "CHECKIN");
    assert_eq!(body_type, 0);
    assert!(body_len > 0);
}
