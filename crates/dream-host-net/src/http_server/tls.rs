use super::state::error;
use crate::socket_operations::Failure;
use std::{io::BufReader, sync::Arc};
use tokio_rustls::{TlsAcceptor, rustls};

pub(super) fn acceptor(cert: &str, key: &str) -> Result<Option<TlsAcceptor>, Failure> {
    if cert.is_empty() && key.is_empty() {
        return Ok(None);
    }
    if cert.is_empty() || key.is_empty() {
        return Err(error(
            "EINVAL",
            "TLS certificate and key paths must be supplied together",
        ));
    }
    let cert = std::fs::File::open(cert).map_err(|e| error("EIO", e))?;
    let key = std::fs::File::open(key).map_err(|e| error("EIO", e))?;
    let certs = rustls_pemfile::certs(&mut BufReader::new(cert))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| error("EINVAL", e))?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(key))
        .map_err(|e| error("EINVAL", e))?
        .ok_or_else(|| error("EINVAL", "no TLS private key found"))?;
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| error("EINVAL", e))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Some(TlsAcceptor::from(Arc::new(config))))
}
