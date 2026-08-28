#[cfg(feature = "crypto")]
use pdf_ast::crypto::get_default_crypto_provider;

#[cfg(feature = "crypto")]
#[test]
fn test_crypto_provider_rc4_roundtrip() {
    let provider = get_default_crypto_provider();
    let key = b"rc4-test-key";
    let data = b"pdf-ast-rc4-roundtrip";

    let encrypted = provider.encrypt(data, key, "RC4").expect("encrypt rc4");
    let decrypted = provider
        .decrypt(&encrypted, key, "RC4")
        .expect("decrypt rc4");

    assert_eq!(decrypted, data);
}

#[cfg(feature = "crypto")]
#[test]
fn test_crypto_provider_aes_128_roundtrip() {
    let provider = get_default_crypto_provider();
    let key = [0x11u8; 16];
    let data = b"pdf-ast-aes-128-roundtrip";

    let encrypted = provider
        .encrypt(data, &key, "AES-128-CBC")
        .expect("encrypt aes-128");
    let decrypted = provider
        .decrypt(&encrypted, &key, "AES-128-CBC")
        .expect("decrypt aes-128");

    assert_eq!(decrypted, data);
}

#[cfg(feature = "crypto")]
#[test]
fn test_crypto_provider_aes_256_roundtrip() {
    let provider = get_default_crypto_provider();
    let key = [0x22u8; 32];
    let data = b"pdf-ast-aes-256-roundtrip";

    let encrypted = provider
        .encrypt(data, &key, "AES-256-CBC")
        .expect("encrypt aes-256");
    let decrypted = provider
        .decrypt(&encrypted, &key, "AES-256-CBC")
        .expect("decrypt aes-256");

    assert_eq!(decrypted, data);
}

#[cfg(not(feature = "crypto"))]
#[test]
fn test_crypto_provider_roundtrip_no_crypto() {}
