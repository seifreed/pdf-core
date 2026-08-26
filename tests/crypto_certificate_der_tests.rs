#[cfg(feature = "crypto")]
use openssl::asn1::Asn1Time;
#[cfg(feature = "crypto")]
use openssl::hash::MessageDigest;
#[cfg(feature = "crypto")]
use openssl::pkey::PKey;
#[cfg(feature = "crypto")]
use openssl::rsa::Rsa;
#[cfg(feature = "crypto")]
use openssl::x509::{X509Builder, X509NameBuilder};

#[cfg(feature = "crypto")]
use pdf_ast::crypto::certificates::parse_der_certificate;

#[cfg(feature = "crypto")]
#[test]
fn test_certificate_der_roundtrip() {
    let rsa = Rsa::generate(2048).expect("rsa key");
    let pkey = PKey::from_rsa(rsa).expect("pkey");

    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_text("CN", "pdf-ast-der").unwrap();
    let name = name.build();

    let mut builder = X509Builder::new().expect("x509 builder");
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder
        .set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::days_from_now(365).unwrap())
        .unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let cert = builder.build();
    let cert_der = cert.to_der().unwrap();

    let info = parse_der_certificate(&cert_der).expect("parse DER");
    assert_eq!(info.der, cert_der);
}

#[cfg(not(feature = "crypto"))]
#[test]
fn test_truncated_certificate_length_is_rejected() {
    let mut cert = vec![0x30, 0x82, 0xff, 0xff];
    cert.resize(100, 0);

    assert!(parse_der_certificate(&cert).is_err());
}
