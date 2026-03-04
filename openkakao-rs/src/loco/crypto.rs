use aes::Aes128;
use anyhow::Result;
use base64::prelude::*;
use byteorder::{LittleEndian, WriteBytesExt};
use cfb_mode::cipher::{AsyncStreamCipher, KeyIvInit};
use rand::RngCore;
use rsa::{oaep, BigUint, RsaPublicKey};
use sha1::Sha1;

type Aes128Cfb = cfb_mode::Encryptor<Aes128>;
type Aes128CfbDec = cfb_mode::Decryptor<Aes128>;

// KakaoTalk's RSA public key (2048-bit, e=3) for LOCO handshake.
// Extracted from /Applications/KakaoTalk.app/Contents/MacOS/KakaoTalk binary.
const LOCO_RSA_PUBLIC_KEY_DER_B64: &str = concat!(
    "MIIBCAKCAQEAo7B26MRFhR8ZpnDCMarG20Lv0JcX0GBIpcxWkGzRqye53zf/1QF+",
    "fBOhQFtdHD5IeaakmdPGGKckcrC1DKXvHvbupwNp2UE/5mLY4rR5qfchQu5wzubCr",
    "RIEXVKyXEogSiiWjjfwumpJ7j7J8qx6ZRhBYPIvYsQ6QGfNjSpvE9m4KYqwAnY9I",
    "2ydGHnX/OW4+pEIgrIeFSR+DQokeRMI5RmDYUQC6foDBXxX6eF4scw5/mcojvxGG",
    "UXLyqEdH8wSPnULhh8NRH6+PBFfQRpC3JXdsh2kJ3SlvLHd9/pfEGKAEMdPNvMcQ",
    "O/P4on9gbq6RKZVamwwEhBBS2Ajw/RjcQIBAw=="
);

// Handshake constants
const HANDSHAKE_KEY_SIZE: u32 = 256; // 2048-bit RSA output
const HANDSHAKE_KEY_ENCRYPT_TYPE: u32 = 16; // RSA-OAEP (kSecPaddingOAEPKey)
const HANDSHAKE_ENCRYPT_TYPE: u32 = 2; // AES-128-CFB

pub struct LocoEncryptor {
    aes_key: [u8; 16],
}

impl LocoEncryptor {
    pub fn new() -> Self {
        let mut key = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut key);
        Self { aes_key: key }
    }

    pub fn build_handshake_packet(&self) -> Result<Vec<u8>> {
        let der_data = BASE64_STANDARD.decode(LOCO_RSA_PUBLIC_KEY_DER_B64)?;
        let public_key = parse_der_rsa_public_key(&der_data)?;

        let mut rng = rand::thread_rng();
        let encrypted_key =
            public_key.encrypt(&mut rng, oaep::Oaep::new::<Sha1>(), &self.aes_key)?;

        let mut buf = Vec::with_capacity(268);
        buf.write_u32::<LittleEndian>(HANDSHAKE_KEY_SIZE)?;
        buf.write_u32::<LittleEndian>(HANDSHAKE_KEY_ENCRYPT_TYPE)?;
        buf.write_u32::<LittleEndian>(HANDSHAKE_ENCRYPT_TYPE)?;
        buf.extend_from_slice(&encrypted_key);

        Ok(buf)
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);

        let mut data = plaintext.to_vec();
        let cipher = Aes128Cfb::new(&self.aes_key.into(), &iv.into());
        cipher.encrypt(&mut data);

        let size = (16 + data.len()) as u32;
        let mut buf = Vec::with_capacity(4 + 16 + data.len());
        buf.write_u32::<LittleEndian>(size).unwrap();
        buf.extend_from_slice(&iv);
        buf.extend_from_slice(&data);
        buf
    }

    pub fn decrypt(&self, data: &[u8]) -> Vec<u8> {
        let iv = &data[..16];
        let mut encrypted = data[16..].to_vec();

        let cipher = Aes128CfbDec::new(&self.aes_key.into(), iv.into());
        cipher.decrypt(&mut encrypted);
        encrypted
    }
}

/// Parse a DER-encoded RSA public key (PKCS#1 format).
fn parse_der_rsa_public_key(der: &[u8]) -> Result<RsaPublicKey> {
    // Try PKCS#1 format first (what KakaoTalk uses)
    if let Ok(key) = rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_der(der) {
        return Ok(key);
    }

    // Fallback: manually parse the DER structure
    let (n_bytes, e_bytes) = parse_der_sequence_two_integers(der)?;
    let n = BigUint::from_bytes_be(n_bytes);
    let e = BigUint::from_bytes_be(e_bytes);
    Ok(RsaPublicKey::new(n, e)?)
}

fn parse_der_sequence_two_integers(der: &[u8]) -> Result<(&[u8], &[u8])> {
    let mut pos = 0;

    if der[pos] != 0x30 {
        anyhow::bail!("Expected SEQUENCE tag 0x30, got 0x{:02x}", der[pos]);
    }
    pos += 1;

    let (_seq_len, consumed) = parse_der_length(&der[pos..])?;
    pos += consumed;

    if der[pos] != 0x02 {
        anyhow::bail!("Expected INTEGER tag 0x02 for n, got 0x{:02x}", der[pos]);
    }
    pos += 1;
    let (n_len, consumed) = parse_der_length(&der[pos..])?;
    pos += consumed;
    let n_bytes = &der[pos..pos + n_len];
    pos += n_len;

    if der[pos] != 0x02 {
        anyhow::bail!("Expected INTEGER tag 0x02 for e, got 0x{:02x}", der[pos]);
    }
    pos += 1;
    let (e_len, consumed) = parse_der_length(&der[pos..])?;
    pos += consumed;
    let e_bytes = &der[pos..pos + e_len];

    Ok((n_bytes, e_bytes))
}

fn parse_der_length(data: &[u8]) -> Result<(usize, usize)> {
    if data[0] < 0x80 {
        Ok((data[0] as usize, 1))
    } else {
        let num_bytes = (data[0] & 0x7F) as usize;
        let mut len = 0usize;
        for i in 0..num_bytes {
            len = (len << 8) | (data[1 + i] as usize);
        }
        Ok((len, 1 + num_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::traits::PublicKeyParts;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let enc = LocoEncryptor::new();
        let plaintext = b"Hello, LOCO protocol!";

        let encrypted = enc.encrypt(plaintext);
        assert!(encrypted.len() > 4 + 16);

        let frame = &encrypted[4..];
        let decrypted = enc.decrypt(frame);
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_handshake_packet_size() {
        let enc = LocoEncryptor::new();
        let packet = enc.build_handshake_packet().unwrap();
        assert_eq!(packet.len(), 268);
    }

    #[test]
    fn test_handshake_packet_header() {
        let enc = LocoEncryptor::new();
        let packet = enc.build_handshake_packet().unwrap();

        let key_size = u32::from_le_bytes(packet[0..4].try_into().unwrap());
        let key_encrypt_type = u32::from_le_bytes(packet[4..8].try_into().unwrap());
        let encrypt_type = u32::from_le_bytes(packet[8..12].try_into().unwrap());

        assert_eq!(key_size, 256);
        assert_eq!(key_encrypt_type, 16);
        assert_eq!(encrypt_type, 2);
    }

    #[test]
    fn test_parse_rsa_public_key() {
        let der_data = BASE64_STANDARD.decode(LOCO_RSA_PUBLIC_KEY_DER_B64).unwrap();
        let key = parse_der_rsa_public_key(&der_data).unwrap();
        assert_eq!(key.e(), &BigUint::from(3u32));
        assert_eq!(key.n().bits(), 2048);
    }

    #[test]
    fn test_different_keys_produce_different_ciphertexts() {
        let enc1 = LocoEncryptor::new();
        let enc2 = LocoEncryptor::new();
        let plaintext = b"same plaintext";

        let ct1 = enc1.encrypt(plaintext);
        let ct2 = enc2.encrypt(plaintext);
        assert_ne!(ct1, ct2);
    }
}
