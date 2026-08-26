use pdf_ast::crypto::CryptoConfig;

#[test]
fn crypto_config_defaults_include_tsa_checks() {
    let config = CryptoConfig::default();
    assert!(!config.enable_tsa_chain_validation);
    assert!(!config.enable_tsa_revocation_checks);
}
