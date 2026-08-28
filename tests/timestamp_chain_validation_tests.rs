#[cfg(feature = "crypto")]
use pdf_ast::crypto::certificates::CertificateChainValidator;
#[cfg(feature = "crypto")]
use pdf_ast::crypto::CryptoConfig;

#[cfg(feature = "crypto")]
#[test]
fn validate_tsa_chain_with_custom_trust_store() {
    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::extension::{BasicConstraints, KeyUsage};
    use openssl::x509::{X509NameBuilder, X509};

    let root_rsa = Rsa::generate(2048).expect("root rsa");
    let root_key = PKey::from_rsa(root_rsa).expect("root key");
    let mut root_name = X509NameBuilder::new().expect("root name");
    root_name
        .append_entry_by_text("CN", "Test Root")
        .expect("root cn");
    let root_name = root_name.build();

    let mut root = X509::builder().expect("root builder");
    root.set_version(2).expect("root version");
    root.set_subject_name(&root_name).expect("root subject");
    root.set_issuer_name(&root_name).expect("root issuer");
    root.set_pubkey(&root_key).expect("root pubkey");
    root.set_not_before(&Asn1Time::days_from_now(0).expect("nb"))
        .expect("nb2");
    root.set_not_after(&Asn1Time::days_from_now(365).expect("na"))
        .expect("na2");
    let basic = BasicConstraints::new().ca().build().expect("basic");
    root.append_extension(basic).expect("basic ext");
    let ku = KeyUsage::new()
        .key_cert_sign()
        .crl_sign()
        .digital_signature()
        .build()
        .expect("ku");
    root.append_extension(ku).expect("ku ext");
    root.sign(&root_key, MessageDigest::sha256())
        .expect("root sign");
    let root_cert = root.build();

    let tsa_rsa = Rsa::generate(2048).expect("tsa rsa");
    let tsa_key = PKey::from_rsa(tsa_rsa).expect("tsa key");
    let mut tsa_name = X509NameBuilder::new().expect("tsa name");
    tsa_name
        .append_entry_by_text("CN", "Test TSA")
        .expect("tsa cn");
    let tsa_name = tsa_name.build();

    let mut tsa = X509::builder().expect("tsa builder");
    tsa.set_version(2).expect("tsa version");
    tsa.set_subject_name(&tsa_name).expect("tsa subject");
    tsa.set_issuer_name(&root_name).expect("tsa issuer");
    tsa.set_pubkey(&tsa_key).expect("tsa pubkey");
    tsa.set_not_before(&Asn1Time::days_from_now(0).expect("nb"))
        .expect("nb2");
    tsa.set_not_after(&Asn1Time::days_from_now(365).expect("na"))
        .expect("na2");
    let ku = KeyUsage::new().digital_signature().build().expect("tsa ku");
    tsa.append_extension(ku).expect("tsa ku ext");
    tsa.sign(&root_key, MessageDigest::sha256())
        .expect("tsa sign");
    let tsa_cert = tsa.build();

    let root_pem = root_cert.to_pem().expect("root pem");
    let temp_dir = std::env::temp_dir();
    let trust_path = temp_dir.join("tsa_root.pem");
    std::fs::write(&trust_path, &root_pem).expect("write trust store");

    let config = CryptoConfig {
        trust_store_path: Some(trust_path.to_string_lossy().to_string()),
        enable_cert_chain_validation: true,
        enable_tsa_chain_validation: true,
        enable_tsa_revocation_checks: true,
        ..Default::default()
    };

    let validator = CertificateChainValidator::new(config).expect("validator");
    let chain = [
        tsa_cert.to_der().expect("tsa der"),
        root_cert.to_der().expect("root der"),
    ];
    let chain_refs: Vec<&[u8]> = chain.iter().map(|c| c.as_slice()).collect();
    let result = validator.validate_chain(&chain_refs).expect("validate");
    assert!(result.is_valid);
}
