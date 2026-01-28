# Test Keys

## ⛔ SECURITY WARNING ⛔

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                    DO NOT USE THESE KEYS IN PRODUCTION                        ║
║                                                                               ║
║  The private key seed is a WELL-KNOWN TEST VALUE and provides NO SECURITY.   ║
║  Anyone can forge signatures using this key.                                  ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## How to Identify Test Keys

The test key_id starts with a recognizable prefix. If you see this key_id in
your `trusted_key_ids` policy configuration, you have a configuration error:

```
key_id: sha256:646d6be49d9f0048f94f67749eca35156eed4f7a7be18e4fc4a94bfd44e300b0
        ^^^^^^
        Begins with "646d6b" = "dkb" in ASCII = "DeterministicKeyBytes"
```

**NEVER whitelist this key_id in production `trusted_key_ids` policy!**

## Test Key Details

| Property | Value |
|----------|-------|
| Algorithm | Ed25519 |
| Seed (hex) | `0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20` |
| Key ID | `sha256:646d6be49d9f0048f94f67749eca35156eed4f7a7be18e4fc4a94bfd44e300b0` |
| Public Key (SPKI DER, base64) | `MCowBQYDK2VwAyEAebVWLo/mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ=` |
| Private Key (PKCS#8 DER, base64) | `MFECAQEwBQYDK2VwBCIEIAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8ggSEAebVWLo/mVPlAeLES6KmLp5AfhTrmlb7X4OORC60ElmQ=` |

## Intended Use

✅ **Use this key for:**
- Golden test vectors
- Cross-language interoperability testing
- CI/CD test fixtures
- Unit tests

❌ **NEVER use for:**
- Production signing
- Staging environments
- Anything security-sensitive
- `trusted_key_ids` in deployed policies

## Verification

To verify you're using the test key correctly in tests:

```rust
const TEST_KEY_SEED: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
    0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];
let key = SigningKey::from_bytes(&TEST_KEY_SEED);
```
