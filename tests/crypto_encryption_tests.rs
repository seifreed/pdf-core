#[cfg(feature = "crypto")]
use pdf_ast::crypto::encryption::{aes_decrypt_cbc, aes_encrypt_cbc};
use pdf_ast::crypto::encryption::{md5, sha256};

#[test]
fn test_sha256_known_vector() {
    let digest = sha256(b"abc");
    let hex = hex::encode(digest);
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_md5_known_vector() {
    let digest = md5(b"abc");
    let hex = hex::encode(digest);
    assert_eq!(hex, "900150983cd24fb0d6963f7d28e17f72");
}

#[test]
#[cfg(feature = "crypto")]
fn test_aes_round_trip_128() {
    let key = [0x11u8; 16];
    let iv = [0x22u8; 16];
    let plaintext = b"PDF-AST AES CBC test payload";

    let encrypted = aes_encrypt_cbc(plaintext, &key, Some(&iv)).expect("encrypt");
    let decrypted = aes_decrypt_cbc(&encrypted, &key, Some(&iv)).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}

#[test]
#[cfg(feature = "crypto")]
fn test_aes_round_trip_256() {
    let key = [0x33u8; 32];
    let iv = [0x44u8; 16];
    let plaintext = b"PDF-AST AES-256 CBC test payload";

    let encrypted = aes_encrypt_cbc(plaintext, &key, Some(&iv)).expect("encrypt");
    let decrypted = aes_decrypt_cbc(&encrypted, &key, Some(&iv)).expect("decrypt");
    assert_eq!(decrypted, plaintext);
}
