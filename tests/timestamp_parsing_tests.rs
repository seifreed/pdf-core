#[cfg(feature = "crypto")]
use pdf_ast::crypto::timestamp;

#[cfg(feature = "crypto")]
#[test]
fn parse_tst_info_from_direct_der() {
    use chrono::{TimeZone, Utc};
    use openssl::hash::{hash, MessageDigest};

    let data = include_bytes!("fixtures/tstinfo_sha256_hello.der");
    let parsed = timestamp::parse_timestamp_token(data).expect("failed to parse TSTInfo");

    assert_eq!(parsed.policy_oid.as_deref(), Some("1.2.3.4"));
    assert_eq!(parsed.hash_algorithm.as_deref(), Some("SHA-256"));
    let expected_ts = Utc
        .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    assert_eq!(parsed.time.timestamp(), expected_ts);

    let expected = hash(MessageDigest::sha256(), b"hello")
        .expect("hash failed")
        .to_vec();
    assert_eq!(parsed.message_imprint.as_deref(), Some(expected.as_slice()));
    assert!(parsed.tsa_certificate_der.is_none());
    assert!(parsed.tsa_chain_der.is_empty());
}
