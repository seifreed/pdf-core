#[cfg(feature = "crypto")]
use openssl::asn1::Asn1Time;
#[cfg(feature = "crypto")]
use openssl::hash::MessageDigest;
#[cfg(feature = "crypto")]
use openssl::pkey::PKey;
#[cfg(feature = "crypto")]
use openssl::rsa::Rsa;
#[cfg(feature = "crypto")]
use openssl::sign::Signer;
#[cfg(feature = "crypto")]
use openssl::x509::{X509Builder, X509NameBuilder};

#[cfg(feature = "crypto")]
use pdf_ast::crypto::signature_verification::verify_rsa_signature_with_cert_der;

#[cfg(feature = "crypto")]
#[test]
fn test_rsa_signature_verification_with_cert_der() {
    let rsa = Rsa::generate(2048).expect("rsa key");
    let pkey = PKey::from_rsa(rsa).expect("pkey");

    let mut name = X509NameBuilder::new().expect("name builder");
    name.append_entry_by_text("CN", "pdf-ast-test").unwrap();
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

    let data = b"pdf-ast-rsa-verification";
    let mut signer = Signer::new(MessageDigest::sha256(), &pkey).unwrap();
    signer.update(data).unwrap();
    let signature = signer.sign_to_vec().unwrap();

    let ok = verify_rsa_signature_with_cert_der(&signature, data, &cert_der, "SHA-256")
        .expect("verify ok");
    assert!(ok);

    let bad = verify_rsa_signature_with_cert_der(&signature, b"tampered", &cert_der, "SHA-256")
        .expect("verify bad");
    assert!(!bad);
}

#[cfg(not(feature = "crypto"))]
#[test]
fn test_rsa_signature_verification_no_crypto() {}
