use rcgen::{CertificateParams, KeyPair};
use sha2::{Digest, Sha256};

pub struct SelfSignedCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub fingerprint: String,
}

pub fn generate() -> anyhow::Result<SelfSignedCert> {
    let mut params = CertificateParams::new(vec!["localhost".into()])?;
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        rcgen::DnValue::Utf8String("shellphone".into()),
    );

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    let cert_der = cert.der();
    let fingerprint = hex_fingerprint(cert_der.as_ref());

    Ok(SelfSignedCert {
        cert_pem,
        key_pem,
        fingerprint,
    })
}

fn hex_fingerprint(der: &[u8]) -> String {
    let hash = Sha256::digest(der);
    hash.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
