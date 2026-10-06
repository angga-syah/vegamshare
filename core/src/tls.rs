//! TLS 1.3 configuration with certificate pinning instead of a CA.
//!
//! Both sides always present their long-lived self-signed certificate and
//! prove possession of its key in the handshake (the handshake signature is
//! verified normally). What is *not* done is CA chain validation: identity
//! is decided by comparing the certificate fingerprint with the one stored
//! at pairing time.
//!
//! * Dialing a paired device: the verifier rejects any certificate whose
//!   fingerprint differs from the pinned one, aborting the handshake.
//! * Dialing for pairing: any certificate is accepted; the password-
//!   authenticated key exchange bound to this TLS session (see `pairing`)
//!   then proves who is on the other end.
//! * Accepting: any client certificate is accepted at the TLS layer and the
//!   request handler checks the fingerprint against the trusted list before
//!   serving anything but ping/pairing.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};

use crate::error::Result;
use crate::identity::{Fingerprint, Identity};
use crate::proto::ALPN;

pub(crate) fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn algorithms() -> WebPkiSupportedAlgorithms {
    rustls::crypto::ring::default_provider().signature_verification_algorithms
}

// Our certificates are always Ed25519; refusing anything else keeps the
// accepted surface minimal.
const SCHEMES: &[SignatureScheme] = &[SignatureScheme::ED25519];

fn verify13(message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
    if dss.scheme != SignatureScheme::ED25519 {
        return Err(rustls::Error::PeerIncompatible(rustls::PeerIncompatible::NoSignatureSchemesInCommon));
    }
    verify_tls13_signature(message, cert, dss, &algorithms())
}

fn tls12_unsupported() -> rustls::Error {
    rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls13RequiredForQuic)
}

#[derive(Debug)]
struct PinnedServerVerifier {
    /// `None` only while pairing.
    expected: Option<Fingerprint>,
}

impl ServerCertVerifier for PinnedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if !intermediates.is_empty() {
            return Err(rustls::Error::General("unexpected certificate chain".into()));
        }
        if let Some(expected) = &self.expected
            && !Fingerprint::of_cert(end_entity).ct_eq(expected)
        {
            return Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(tls12_unsupported())
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

#[derive(Debug)]
struct AnyClientCert;

impl ClientCertVerifier for AnyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn verify_client_cert(&self, _end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], _now: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        if !intermediates.is_empty() {
            return Err(rustls::Error::General("unexpected certificate chain".into()));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(tls12_unsupported())
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

pub(crate) fn server_config(id: &Identity) -> Result<rustls::ServerConfig> {
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(Arc::new(AnyClientCert))
        .with_single_cert(vec![id.cert.clone()], id.private_key())?;
    cfg.alpn_protocols = vec![ALPN.to_vec()];
    // Every connection must present and prove the pinned certificate, so
    // session resumption is disabled on this side too.
    cfg.send_tls13_tickets = 0;
    cfg.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
    Ok(cfg)
}

pub(crate) fn client_config(id: &Identity, expected: Option<Fingerprint>) -> Result<rustls::ClientConfig> {
    let mut cfg = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedServerVerifier { expected }))
        .with_client_auth_cert(vec![id.cert.clone()], id.private_key())?;
    cfg.alpn_protocols = vec![ALPN.to_vec()];
    // Session resumption would skip certificate presentation; always do a
    // full handshake so the pinned certificate is checked every time.
    cfg.resumption = rustls::client::Resumption::disabled();
    Ok(cfg)
}

/// Fingerprint of the certificate the peer presented on `conn`.
pub(crate) fn peer_fingerprint(conn: &quinn::Connection) -> Option<Fingerprint> {
    let certs = conn.peer_identity()?.downcast::<Vec<CertificateDer<'static>>>().ok()?;
    certs.first().map(|c| Fingerprint::of_cert(c))
}
