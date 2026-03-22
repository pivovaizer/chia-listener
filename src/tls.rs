use crate::error::ChiaError;
use chia_ssl::{ChiaCertificate, CHIA_CA_CRT};
use native_tls::{Certificate, Identity, TlsConnector};
use std::fs;

/// Loads or generates SSL certificates for Chia connections
pub fn load_or_generate_cert() -> Result<ChiaCertificate, ChiaError> {
    // Use a fixed location for certificates
    let cert_dir = dirs::home_dir()
        .ok_or_else(|| ChiaError::Other("Could not find home directory".to_string()))?
        .join(".chia-block-listener")
        .join("ssl");

    fs::create_dir_all(&cert_dir).map_err(ChiaError::Io)?;

    let cert_path = cert_dir.join("client.crt");
    let key_path = cert_dir.join("client.key");

    // Try to load existing certificates
    if cert_path.exists() && key_path.exists() {
        let cert_pem = fs::read_to_string(&cert_path).map_err(ChiaError::Io)?;
        let key_pem = fs::read_to_string(&key_path).map_err(ChiaError::Io)?;

        Ok(ChiaCertificate { cert_pem, key_pem })
    } else {
        // Generate new certificates
        let cert = ChiaCertificate::generate()
            .map_err(|e| ChiaError::Other(format!("Failed to generate certificate: {e}")))?;

        // Save for future use
        fs::write(&cert_path, &cert.cert_pem).map_err(ChiaError::Io)?;
        fs::write(&key_path, &cert.key_pem).map_err(ChiaError::Io)?;

        Ok(cert)
    }
}

/// Creates a native-tls connector from a Chia certificate
pub fn create_tls_connector(cert: &ChiaCertificate) -> Result<TlsConnector, ChiaError> {
    let identity = Identity::from_pkcs8(cert.cert_pem.as_bytes(), cert.key_pem.as_bytes())
        .map_err(|e| ChiaError::Tls(format!("Failed to create identity: {e}")))?;

    let ca_cert = Certificate::from_pem(CHIA_CA_CRT.as_bytes())
        .map_err(|e| ChiaError::Tls(format!("Failed to parse CA certificate: {e}")))?;

    let tls_connector = TlsConnector::builder()
        .identity(identity)
        .add_root_certificate(ca_cert)
        .danger_accept_invalid_certs(true) // Accept self-signed certificates
        .build()
        .map_err(|e| ChiaError::Tls(format!("Failed to build TLS connector: {e}")))?;

    Ok(tls_connector)
}
