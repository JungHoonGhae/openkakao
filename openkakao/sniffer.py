"""Capture LOCO protocol traffic from KakaoTalk using tcpdump + TLS key logging.

Since KakaoTalk uses TLS (V2SL mode), we need to either:
1. Use SSLKEYLOGFILE to capture TLS pre-master secrets (requires app support)
2. Use Frida to hook the crypto functions and extract keys
3. Hook the LOCO layer directly (post-TLS decryption)

This module provides a Frida-based approach to intercept LOCO packets
after TLS decryption, extracting the OAuth token from LOGINLIST.
"""

import json
import os
import signal
import struct
import subprocess
import sys
import tempfile

FRIDA_SCRIPT = r"""
// Frida script to intercept LOCO packets in KakaoTalk Mac

// Hook LocoAgent's send/receive methods
var locoAgentClass = ObjC.classes['LocoAgent'];
if (locoAgentClass) {
    console.log("[*] Found LocoAgent class");

    // Find and hook the packet sending method
    var methods = locoAgentClass.$methods;
    methods.forEach(function(method) {
        if (method.indexOf('send') !== -1 || method.indexOf('packet') !== -1 ||
            method.indexOf('write') !== -1 || method.indexOf('request') !== -1) {
            console.log("  Method: " + method);
        }
    });
}

// Hook BCLocoClient for higher-level command interception
var bcLocoClient = ObjC.classes['BCLocoClient'];
if (bcLocoClient) {
    console.log("[*] Found BCLocoClient class");
    var methods = bcLocoClient.$methods;
    methods.forEach(function(method) {
        if (method.indexOf('login') !== -1 || method.indexOf('Login') !== -1 ||
            method.indexOf('token') !== -1 || method.indexOf('Token') !== -1 ||
            method.indexOf('auth') !== -1 || method.indexOf('oauth') !== -1) {
            console.log("  Auth method: " + method);
        }
    });
}

// Hook LocoManager for connection management
var locoManager = ObjC.classes['LocoManager'];
if (locoManager) {
    console.log("[*] Found LocoManager class");
}

// Hook LocoPacketProducer to intercept outgoing packets
var packetProducer = ObjC.classes['LocoPacketProducer'];
if (packetProducer) {
    console.log("[*] Found LocoPacketProducer class");
    var methods = packetProducer.$methods;
    methods.forEach(function(method) {
        console.log("  " + method);
    });
}

// Hook NSData+Crypto for encryption operations
// This lets us see data before encryption
Interceptor.attach(ObjC.classes['NSData']['- dataByAES128CFBEncrypting'].implementation, {
    onEnter: function(args) {
        var nsdata = ObjC.Object(args[0]);
        var bytes = nsdata.bytes();
        var length = nsdata.length();
        if (length > 22) {
            // Try to parse as LOCO packet
            var method = Memory.readUtf8String(bytes.add(6), 11);
            console.log("[LOCO OUT] Method: " + method + " Length: " + length);

            // Read body if present
            if (length > 22) {
                var bodyLen = Memory.readU32(bytes.add(18));
                console.log("  Body length: " + bodyLen);
                // Dump first 200 bytes of body as hex
                var bodyBytes = Memory.readByteArray(bytes.add(22), Math.min(bodyLen, 200));
                console.log("  Body: " + hexdump(bodyBytes));
            }
        }
    }
});

console.log("[*] KakaoTalk LOCO interceptor loaded");
"""


def create_frida_script() -> str:
    """Write the Frida script to a temp file."""
    path = os.path.join(tempfile.gettempdir(), "kakao_loco_intercept.js")
    with open(path, "w") as f:
        f.write(FRIDA_SCRIPT)
    return path


def check_frida() -> bool:
    """Check if Frida is installed."""
    try:
        result = subprocess.run(["frida", "--version"], capture_output=True, text=True)
        return result.returncode == 0
    except FileNotFoundError:
        return False


def run_sniffer():
    """Run the LOCO traffic sniffer."""
    if not check_frida():
        print("Frida is not installed. Install it with: pip install frida-tools")
        print()
        print("Alternative: Use tcpdump to capture raw traffic:")
        print("  sudo tcpdump -i en0 host 211.183.215.108 -w kakao_capture.pcap")
        print()
        print("Or install mitmproxy and set SSLKEYLOGFILE:")
        print("  export SSLKEYLOGFILE=/tmp/kakao_keys.log")
        print("  mitmproxy --mode transparent --tcp-hosts '.*kakao.*'")
        return

    script_path = create_frida_script()
    print(f"Frida script: {script_path}")

    # Find KakaoTalk PID
    try:
        result = subprocess.run(
            ["pgrep", "-x", "KakaoTalk"],
            capture_output=True, text=True,
        )
        pid = result.stdout.strip()
        if not pid:
            print("KakaoTalk is not running")
            return

        print(f"Attaching to KakaoTalk (PID {pid})...")
        subprocess.run(["frida", "-p", pid, "-l", script_path])
    except KeyboardInterrupt:
        print("\nStopped")


if __name__ == "__main__":
    run_sniffer()
