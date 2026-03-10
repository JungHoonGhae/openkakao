use openkakao_rs::loco::packet::{LocoPacket, PacketBuilder, HEADER_SIZE};

#[test]
fn packet_builder_increments_id() {
    let builder = PacketBuilder::new();
    let p1 = builder.build("TEST1", bson::doc! {});
    let p2 = builder.build("TEST2", bson::doc! {});
    assert!(p2.packet_id > p1.packet_id);
}

#[test]
fn packet_builder_starts_at_one() {
    let builder = PacketBuilder::new();
    let pkt = builder.build("FIRST", bson::doc! {});
    assert_eq!(pkt.packet_id, 1);
}

#[test]
fn packet_builder_encode_decode_roundtrip() {
    let builder = PacketBuilder::new();
    let original = builder.build("PING", bson::doc! { "key": "value" });
    let encoded = original.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "PING");
    assert_eq!(decoded.packet_id, original.packet_id);
    assert_eq!(decoded.body.get_str("key").unwrap(), "value");
}

#[test]
fn packet_status_from_i32_body() {
    let builder = PacketBuilder::new();
    let pkt = builder.build("TEST", bson::doc! { "status": 0_i32 });
    assert_eq!(pkt.status(), 0);
}

#[test]
fn packet_status_from_i64_body() {
    let builder = PacketBuilder::new();
    let pkt = builder.build("TEST", bson::doc! { "status": -950_i64 });
    assert_eq!(pkt.status(), -950);
}

#[test]
fn packet_status_falls_back_to_header() {
    let pkt = LocoPacket {
        packet_id: 1,
        status_code: -1,
        method: "FAIL".to_string(),
        body_type: 0,
        body: bson::doc! {},
    };
    assert_eq!(pkt.status(), -1);
}

#[test]
fn empty_body_packet_roundtrip() {
    let builder = PacketBuilder::new();
    let pkt = builder.build("PING", bson::doc! {});
    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.method, "PING");
    assert!(decoded.body.is_empty());
}

#[test]
fn large_body_packet_roundtrip() {
    let builder = PacketBuilder::new();
    let large_str = "x".repeat(10000);
    let pkt = builder.build("DATA", bson::doc! { "payload": &large_str });
    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    assert_eq!(decoded.body.get_str("payload").unwrap(), large_str);
}

#[test]
fn nested_bson_document_roundtrip() {
    let builder = PacketBuilder::new();
    let inner = bson::doc! { "nested_key": "nested_value" };
    let pkt = builder.build("COMPLEX", bson::doc! { "outer": inner });
    let encoded = pkt.encode();
    let decoded = LocoPacket::decode(&encoded).unwrap();
    let outer = decoded.body.get_document("outer").unwrap();
    assert_eq!(outer.get_str("nested_key").unwrap(), "nested_value");
}

#[test]
fn header_size_is_22() {
    assert_eq!(HEADER_SIZE, 22);
}

#[test]
fn encoded_packet_length_includes_header_and_body() {
    let builder = PacketBuilder::new();
    let pkt = builder.build("TEST", bson::doc! { "a": 1_i32 });
    let encoded = pkt.encode();
    assert!(encoded.len() > HEADER_SIZE);

    let body_bytes = bson::to_vec(&pkt.body).unwrap();
    assert_eq!(encoded.len(), HEADER_SIZE + body_bytes.len());
}

#[test]
fn multiple_builders_are_independent() {
    let b1 = PacketBuilder::new();
    let b2 = PacketBuilder::new();

    let p1 = b1.build("A", bson::doc! {});
    let p2 = b2.build("B", bson::doc! {});

    // Both should start at 1 independently
    assert_eq!(p1.packet_id, 1);
    assert_eq!(p2.packet_id, 1);
}

#[test]
fn packet_builder_default_trait() {
    let builder = PacketBuilder::default();
    let pkt = builder.build("DEFAULT", bson::doc! {});
    assert_eq!(pkt.packet_id, 1);
    assert_eq!(pkt.method, "DEFAULT");
}
