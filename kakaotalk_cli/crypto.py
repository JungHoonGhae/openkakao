"""LOCO protocol encryption: AES-128-CFB + RSA key exchange."""

import os
import struct

from Crypto.Cipher import AES, PKCS1_OAEP
from Crypto.Hash import SHA1
from Crypto.PublicKey import RSA

# KakaoTalk's RSA public key (2048-bit, e=3) for LOCO handshake.
# Extracted from /Applications/KakaoTalk.app/Contents/MacOS/KakaoTalk binary.
LOCO_RSA_PUBLIC_KEY_DER_B64 = (
    "MIIBCAKCAQEAo7B26MRFhR8ZpnDCMarG20Lv0JcX0GBIpcxWkGzRqye53zf/1QF+"
    "fBOhQFtdHD5IeaakmdPGGKckcrC1DKXvHvbupwNp2UE/5mLY4rR5qfchQu5wzubCr"
    "RIEXVKyXEogSiiWjjfwumpJ7j7J8qx6ZRhBYPIvYsQ6QGfNjSpvE9m4KYqwAnY9I"
    "2ydGHnX/OW4+pEIgrIeFSR+DQokeRMI5RmDYUQC6foDBXxX6eF4scw5/mcojvxGG"
    "UXLyqEdH8wSPnULhh8NRH6+PBFfQRpC3JXdsh2kJ3SlvLHd9/pfEGKAEMdPNvMcQ"
    "O/P4on9gbq6RKZVamwwEhBBS2Ajw/RjcQIBAw=="
)

# Handshake constants
HANDSHAKE_KEY_SIZE = 256  # 256 bytes (2048-bit RSA output size)
HANDSHAKE_KEY_ENCRYPT_TYPE = 16  # RSA-OAEP (kSecPaddingOAEPKey)
HANDSHAKE_ENCRYPT_TYPE = 2  # AES-128-CFB


class LocoEncryptor:
    """Handles LOCO protocol AES-128-CFB encryption with RSA key exchange."""

    def __init__(self):
        self.aes_key = os.urandom(16)

    def build_handshake_packet(self) -> bytes:
        """Build RSA handshake packet to send to the LOCO server.

        Returns the 268-byte handshake packet:
        - 4 bytes: key size (256 for 2048-bit RSA)
        - 4 bytes: key encrypt type (16 = kSecPaddingOAEPKey)
        - 4 bytes: encrypt type (2 = AES-CFB)
        - 256 bytes: RSA-encrypted AES key
        """
        import base64
        der_data = base64.b64decode(LOCO_RSA_PUBLIC_KEY_DER_B64)
        rsa_key = RSA.import_key(der_data)
        cipher = PKCS1_OAEP.new(rsa_key, hashAlgo=SHA1)
        encrypted_key = cipher.encrypt(self.aes_key)

        return struct.pack(
            "<III",
            HANDSHAKE_KEY_SIZE,
            HANDSHAKE_KEY_ENCRYPT_TYPE,
            HANDSHAKE_ENCRYPT_TYPE,
        ) + encrypted_key

    def encrypt(self, plaintext: bytes) -> bytes:
        """Encrypt a LOCO command packet.

        Returns:
            Secure frame: 4-byte size + 16-byte IV + encrypted data
        """
        iv = os.urandom(16)
        cipher = AES.new(self.aes_key, AES.MODE_CFB, iv=iv, segment_size=128)
        encrypted = cipher.encrypt(plaintext)
        size = len(iv) + len(encrypted)
        return struct.pack("<I", size) + iv + encrypted

    def decrypt(self, data: bytes) -> bytes:
        """Decrypt a LOCO secure frame.

        Args:
            data: Raw bytes starting after the 4-byte size field,
                  containing 16-byte IV followed by encrypted data.
        """
        iv = data[:16]
        encrypted = data[16:]
        cipher = AES.new(self.aes_key, AES.MODE_CFB, iv=iv, segment_size=128)
        return cipher.decrypt(encrypted)
