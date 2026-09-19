//! Загрузка и получение шаблона сертификата dest для REALITY ImpersonateCert.
//!
//! **Безопасность:** `CaptureVerifier` намеренно не проверяет PKIX/HMAC — он используется
//! только при однократном подключении к публичному `dest` для снятия leaf-сертификата.
//! Для REALITY-клиента к прокси нужен [`rustls::reality::RealityServerCertVerifier`]
//! (проверка HMAC-SHA512 хвоста), а не «доверять всему».

use anyhow::{bail, Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::debug;

/// Загрузить leaf-сертификат из PEM или DER файла.
pub fn load_impersonate_cert_file(path: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(path)
        .with_context(|| format!("cannot read impersonate cert file {}", path))?;
    if Path::new(path).extension().is_some_and(|ext| ext == "pem")
        || data.starts_with(b"-----BEGIN")
    {
        let certs = CertificateDer::pem_slice_iter(&data)
            .collect::<Result<Vec<_>, _>>()
            .context("invalid PEM in impersonate cert file")?;
        let first = certs
            .first()
            .context("impersonate cert PEM file contains no certificates")?;
        return Ok(first.as_ref().to_vec());
    }
    if data.is_empty() {
        bail!("impersonate cert file is empty");
    }
    Ok(data)
}

/// Получить leaf-сертификат с `dest` через TLS (без проверки цепочки).
pub async fn fetch_impersonate_cert_from_dest(dest: &str) -> Result<Vec<u8>> {
    let (host, port) = parse_host_port(dest)?;
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("cannot connect to dest {} for impersonate cert fetch", addr))?;

    let captured = Arc::new(Mutex::new(None));
    let verifier = CaptureVerifier {
        captured: Arc::clone(&captured),
    };

    let provider = rustls::crypto::ring::default_provider();
    let config = ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .context("unsupported TLS protocol versions")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();

    let server_name = ServerName::try_from(host.clone())
        .map_err(|_| anyhow::anyhow!("invalid dest host for TLS SNI: {}", host))?;
    let connector = TlsConnector::from(Arc::new(config));
    let mut tls = connector
        .connect(server_name, stream)
        .await
        .context("TLS handshake to dest failed while fetching impersonate cert")?;

    // Дождаться завершения handshake и закрыть соединение.
    let _ = shutdown_tls(&mut tls).await;
    drop(tls);

    let cert = captured
        .lock()
        .unwrap()
        .clone()
        .context("dest TLS handshake did not provide a leaf certificate")?;
    debug!(
        dest,
        cert_len = cert.len(),
        "fetched REALITY impersonate cert"
    );
    Ok(cert)
}

fn parse_host_port(dest: &str) -> Result<(String, u16)> {
    let (host, port) = match dest.rsplit_once(':') {
        Some((host, port_str)) if !host.is_empty() => {
            let port = port_str
                .parse()
                .with_context(|| format!("invalid dest port in {}", dest))?;
            (host.to_string(), port)
        }
        _ => bail!("dest must be host:port, got {}", dest),
    };
    Ok((host, port))
}

async fn shutdown_tls<S>(stream: &mut tokio_rustls::client::TlsStream<S>) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    stream.shutdown().await?;
    Ok(())
}

/// Только для `fetch_impersonate_cert_from_dest` — не использовать на data plane REALITY.
#[derive(Debug)]
struct CaptureVerifier {
    captured: Arc<Mutex<Option<Vec<u8>>>>,
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.captured.lock().unwrap() = Some(end_entity.as_ref().to_vec());
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::generate_simple_self_signed;

    #[test]
    fn load_pem_impersonate_cert() {
        let cert = generate_simple_self_signed(vec!["example.com".to_string()]).unwrap();
        let pem = cert.cert.pem();
        let dir =
            std::env::temp_dir().join(format!("skadi-impersonate-{}.pem", std::process::id()));
        std::fs::write(&dir, pem).unwrap();
        let der = load_impersonate_cert_file(dir.to_str().unwrap()).unwrap();
        assert!(!der.is_empty());
        let _ = std::fs::remove_file(dir);
    }
}
