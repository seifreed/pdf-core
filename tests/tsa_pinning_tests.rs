#[cfg(feature = "crypto")]
use pdf_ast::crypto::certificates::parse_der_certificate;
#[cfg(feature = "crypto")]
use pdf_ast::crypto::signature_verification::check_tsa_pinning_for_test;
#[cfg(feature = "crypto")]
use pdf_ast::crypto::CryptoConfig;

#[cfg(feature = "crypto")]
fn make_test_cert_fingerprint() -> String {
    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::{X509NameBuilder, X509};

    let rsa = Rsa::generate(2048).expect("rsa");
    let key = PKey::from_rsa(rsa).expect("key");
    let mut name = X509NameBuilder::new().expect("name");
    name.append_entry_by_text("CN", "Pin Test").expect("cn");
    let name = name.build();

    let mut cert = X509::builder().expect("builder");
    cert.set_version(2).expect("version");
    cert.set_subject_name(&name).expect("subject");
    cert.set_issuer_name(&name).expect("issuer");
    cert.set_pubkey(&key).expect("pubkey");
    cert.set_not_before(&Asn1Time::days_from_now(0).expect("nb"))
        .expect("nb2");
    cert.set_not_after(&Asn1Time::days_from_now(365).expect("na"))
        .expect("na2");
    cert.sign(&key, MessageDigest::sha256()).expect("sign");
    let cert = cert.build();
    let der = cert.to_der().expect("der");
    let parsed = parse_der_certificate(&der).expect("parse");
    parsed.fingerprint_sha256
}

#[cfg(feature = "crypto")]
#[test]
fn tsa_pinning_allow_list() {
    let fingerprint = make_test_cert_fingerprint();
    let config = CryptoConfig {
        tsa_allow_fingerprints: vec![fingerprint.clone()],
        ..Default::default()
    };
    let result = check_tsa_pinning_for_test(&config, &fingerprint);
    assert_eq!(result, Some((true, None)));
}

#[cfg(feature = "crypto")]
#[test]
fn tsa_pinning_allow_list_rejects_unknown() {
    let fingerprint = make_test_cert_fingerprint();
    let config = CryptoConfig {
        tsa_allow_fingerprints: vec!["deadbeef".to_string()],
        ..Default::default()
    };
    let result = check_tsa_pinning_for_test(&config, &fingerprint);
    assert!(matches!(result, Some((false, Some(_)))));
}

#[cfg(feature = "crypto")]
#[test]
fn tsa_pinning_block_list_rejects() {
    let fingerprint = make_test_cert_fingerprint();
    let config = CryptoConfig {
        tsa_block_fingerprints: vec![fingerprint.clone()],
        ..Default::default()
    };
    let result = check_tsa_pinning_for_test(&config, &fingerprint);
    assert!(matches!(result, Some((false, Some(_)))));
}
