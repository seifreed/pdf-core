#[cfg(feature = "crypto")]
use pdf_ast::crypto::timestamp;

#[cfg(feature = "crypto")]
#[test]
fn verify_cms_timestamp_signature_and_extract_tsa_cert() {
    use openssl::asn1::Asn1Time;
    use openssl::cms::{CMSOptions, CmsContentInfo};
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::{X509NameBuilder, X509};

    let tst_info = include_bytes!("fixtures/tstinfo_sha256_hello.der");

    let rsa = Rsa::generate(2048).expect("rsa");
    let pkey = PKey::from_rsa(rsa).expect("pkey");
    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_text("CN", "Test TSA").expect("cn");
    let name = name.build();

    let mut builder = X509::builder().expect("builder");
    builder.set_version(2).expect("version");
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(&name).expect("issuer");
    builder.set_pubkey(&pkey).expect("pubkey");
    builder
        .set_not_before(&Asn1Time::days_from_now(0).expect("nb"))
        .expect("nb2");
    builder
        .set_not_after(&Asn1Time::days_from_now(365).expect("na"))
        .expect("na2");
    builder.sign(&pkey, MessageDigest::sha256()).expect("sign");
    let cert = builder.build();

    let cms = CmsContentInfo::sign(
        Some(&cert),
        Some(&pkey),
        None,
        Some(tst_info),
        CMSOptions::BINARY,
    )
    .expect("cms sign");
    let token = cms.to_der().expect("cms der");

    let parsed = timestamp::parse_timestamp_token(&token).expect("parse timestamp");
    assert!(parsed.tsa_certificate_der.is_some());
    assert_eq!(parsed.tsa_chain_der.len(), 1);

    let content = timestamp::verify_timestamp_signature(&token).expect("verify signature");
    assert_eq!(content, tst_info);
}
