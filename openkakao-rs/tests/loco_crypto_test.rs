use openkakao_rs::loco::crypto::LocoEncryptor;

#[test]
fn new_generates_valid_encryptor() {
    let enc = LocoEncryptor::new();
    // Should not panic
    let _ = enc.encrypt(b"test");
}

#[test]
fn encrypt_decrypt_roundtrip_short() {
    let enc = LocoEncryptor::new();
    let plaintext = b"Hello, LOCO!";

    let encrypted = enc.encrypt(plaintext);
    let frame = &encrypted[4..]; // skip size prefix
    let decrypted = enc.decrypt(frame).unwrap();
    assert_eq!(&decrypted, plaintext);
}

#[test]
fn encrypt_decrypt_roundtrip_empty() {
    let enc = LocoEncryptor::new();
    let plaintext = b"";

    let encrypted = enc.encrypt(plaintext);
    let frame = &encrypted[4..];
    let decrypted = enc.decrypt(frame).unwrap();
    assert_eq!(&decrypted, plaintext);
}

#[test]
fn encrypt_decrypt_roundtrip_large() {
    let enc = LocoEncryptor::new();
    let plaintext = vec![0xABu8; 100_000]; // 100KB

    let encrypted = enc.encrypt(&plaintext);
    let frame = &encrypted[4..];
    let decrypted = enc.decrypt(frame).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn encrypt_decrypt_roundtrip_bson_payload() {
    let enc = LocoEncryptor::new();
    let doc = bson::doc! {
        "chatId": 382416827148557_i64,
        "msg": "test message",
        "type": 1_i32,
    };
    let plaintext = bson::to_vec(&doc).unwrap();

    let encrypted = enc.encrypt(&plaintext);
    let frame = &encrypted[4..];
    let decrypted = enc.decrypt(frame).unwrap();
    assert_eq!(decrypted, plaintext);

    // Verify we can parse the BSON back
    let decoded: bson::Document = bson::from_slice(&decrypted).unwrap();
    assert_eq!(decoded.get_str("msg").unwrap(), "test message");
}

#[test]
fn handshake_packet_is_268_bytes() {
    let enc = LocoEncryptor::new();
    let packet = enc.build_handshake_packet().unwrap();
    assert_eq!(packet.len(), 268);
}

#[test]
fn handshake_header_constants() {
    let enc = LocoEncryptor::new();
    let packet = enc.build_handshake_packet().unwrap();

    let key_size = u32::from_le_bytes(packet[0..4].try_into().unwrap());
    let key_encrypt_type = u32::from_le_bytes(packet[4..8].try_into().unwrap());
    let encrypt_type = u32::from_le_bytes(packet[8..12].try_into().unwrap());

    assert_eq!(key_size, 256); // RSA-2048 output
    assert_eq!(key_encrypt_type, 16); // RSA-OAEP SHA-1
    assert_eq!(encrypt_type, 3); // AES-128-GCM
}

#[test]
fn wrong_key_decryption_fails() {
    let enc1 = LocoEncryptor::new();
    let enc2 = LocoEncryptor::new();
    let plaintext = b"secret data";

    let encrypted = enc1.encrypt(plaintext);
    let frame = &encrypted[4..];

    // Decrypting with a different key should fail
    assert!(enc2.decrypt(frame).is_err());
}

#[test]
fn tampered_ciphertext_fails() {
    let enc = LocoEncryptor::new();
    let plaintext = b"sensitive data";
    let mut encrypted = enc.encrypt(plaintext);

    // Tamper with ciphertext (flip a byte after nonce)
    let tamper_idx = 4 + 12 + 1; // size(4) + nonce(12) + 1
    encrypted[tamper_idx] ^= 0xFF;

    let frame = &encrypted[4..];
    assert!(enc.decrypt(frame).is_err());
}

#[test]
fn too_short_data_fails() {
    let enc = LocoEncryptor::new();
    // GCM needs at least nonce(12) + tag(16) = 28 bytes
    let short_data = vec![0u8; 20];
    assert!(enc.decrypt(&short_data).is_err());
}

#[test]
fn different_encryptors_produce_different_ciphertexts() {
    let enc1 = LocoEncryptor::new();
    let enc2 = LocoEncryptor::new();
    let plaintext = b"same plaintext";

    let ct1 = enc1.encrypt(plaintext);
    let ct2 = enc2.encrypt(plaintext);
    assert_ne!(ct1, ct2);
}

#[test]
fn gcm_frame_structure() {
    let enc = LocoEncryptor::new();
    let plaintext = b"test";
    let encrypted = enc.encrypt(plaintext);

    // Read size prefix
    let size = u32::from_le_bytes(encrypted[0..4].try_into().unwrap()) as usize;
    // size = nonce(12) + ciphertext(4) + tag(16) = 32
    assert_eq!(size, 12 + plaintext.len() + 16);
    assert_eq!(encrypted.len(), 4 + size);
}
