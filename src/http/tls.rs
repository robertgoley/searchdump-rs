//! rustls client configuration: custom CA bundles, `--insecure`, and client
//! certificate identities (including passphrase-protected PKCS#8 keys).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

/// TLS material for one endpoint (input or output).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct TlsOptions {
    /// PEM CA bundle; replaces the system roots (Node `ca` semantics).
    pub ca: Option<PathBuf>,
    /// PEM client certificate chain.
    pub cert: Option<PathBuf>,
    /// PEM private key (PKCS#8, PKCS#1/RSA, SEC1, or encrypted PKCS#8).
    pub key: Option<PathBuf>,
    /// Passphrase for an encrypted PKCS#8 key.
    pub pass: Option<String>,
    /// Skip server certificate verification entirely.
    pub insecure: bool,
}

/// Manual `Debug`: the key passphrase is never printed (F-08).
impl std::fmt::Debug for TlsOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsOptions")
            .field("ca", &self.ca)
            .field("cert", &self.cert)
            .field("key", &self.key)
            .field("pass", &self.pass.as_ref().map(|_| "***"))
            .field("insecure", &self.insecure)
            .finish()
    }
}

/// L-809a: the OpenDAL storage path builds its own (reqwest 0.13) TLS client, which takes
/// the process-wide default provider; make that the same ring provider used here.
pub fn install_default_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub fn build_client_config(opts: &TlsOptions) -> Result<ClientConfig> {
    let builder = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .context("Failed to select TLS protocol versions")?;

    let builder = if opts.insecure {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerification(provider())))
    } else {
        builder.with_root_certificates(root_store(opts.ca.as_deref())?)
    };

    let config = match (&opts.cert, &opts.key) {
        (Some(cert), Some(key)) => {
            let chain = load_certs(cert)?;
            let key_path = key;
            let key = load_key(key, opts.pass.as_deref())?;
            builder
                .with_client_auth_cert(chain, key)
                .with_context(|| {
                    format!(
                        "Invalid client certificate/key pair ('{}' and '{}': does the key match the certificate?)",
                        cert.display(),
                        key_path.display()
                    )
                })?
        }
        (Some(_), None) => bail!("A client certificate (--cert) requires a private key (--key)"),
        (None, Some(_)) => bail!("A private key (--key) requires a client certificate (--cert)"),
        (None, None) => builder.with_no_client_auth(),
    };
    Ok(config)
}

fn root_store(ca: Option<&Path>) -> Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    match ca {
        Some(path) => {
            let certs = load_certs(path)?;
            let (added, _ignored) = store.add_parsable_certificates(certs);
            if added == 0 {
                bail!("No usable CA certificates found in '{}'", path.display());
            }
        }
        None => {
            let native = rustls_native_certs::load_native_certs();
            // an explicit SSL_CERT_FILE/SSL_CERT_DIR that cannot be used is worth a warning:
            // a silent switch to other roots surprises users of private CAs (TST-BRK-TLS-004)
            let explicit: Vec<String> = ["SSL_CERT_FILE", "SSL_CERT_DIR"]
                .iter()
                .filter_map(|v| std::env::var(v).ok().map(|x| format!("{v}={x}")))
                .collect();
            for err in &native.errors {
                if explicit.is_empty() {
                    log::debug!("Skipping unreadable system certificate: {err}");
                } else {
                    log::warn!(
                        "Skipping unusable certificate from {}: {err}",
                        explicit.join(", ")
                    );
                }
            }
            let (added, _) = store.add_parsable_certificates(native.certs);
            if added == 0 {
                if explicit.is_empty() {
                    log::debug!("No system CA certificates found; using bundled webpki roots");
                } else {
                    log::warn!(
                        "No usable CA certificates from {}; using the bundled webpki (Mozilla) roots instead. For a private CA pass --ca/--input-ca/--output-ca",
                        explicit.join(", ")
                    );
                }
                store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            }
        }
    }
    Ok(store)
}

fn read(path: &Path, what: &str) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("Failed to read {what} '{}'", path.display()))
}

pub fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let pem = read(path, "certificate file")?;
    let certs = CertificateDer::pem_slice_iter(&pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to parse PEM certificates in '{}'", path.display()))?;
    if certs.is_empty() {
        bail!(
            "No PEM certificates found in '{}'{}",
            path.display(),
            not_pem_hint(&pem, path, "x509")
        );
    }
    if String::from_utf8_lossy(&pem).contains("PRIVATE KEY-----") {
        log::warn!(
            "'{}' also contains a private key; only its certificates are used. A certificate or CA file should not carry keys: keep keys in a separate file with restricted permissions",
            path.display()
        );
    }
    Ok(certs)
}

/// Why a file has no PEM items (TST-BRK-TLS-002): empty, or binary DER (with the openssl
/// command that converts it).
fn not_pem_hint(bytes: &[u8], path: &Path, tool: &str) -> String {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        " (the file is empty)".to_string()
    } else if bytes.first() == Some(&0x30) {
        format!(
            " (it looks like binary DER, not PEM; convert it with `openssl {tool} -inform DER -in {} -out converted.pem`)",
            path.display()
        )
    } else {
        String::new()
    }
}

#[cfg(feature = "encrypted-keys")]
fn decrypt_pkcs8(path: &Path, text: &str, pass: &str) -> Result<PrivateKeyDer<'static>> {
    let (label, doc) = pkcs8::SecretDocument::from_pem(text)
        .map_err(|e| anyhow!("Failed to decode encrypted key '{}': {e}", path.display()))?;
    if label != "ENCRYPTED PRIVATE KEY" {
        bail!("Unexpected PEM label '{label}' in '{}'", path.display());
    }
    let info = pkcs8::EncryptedPrivateKeyInfoRef::try_from(doc.as_bytes())
        .map_err(|e| anyhow!("Invalid encrypted PKCS#8 key '{}': {e}", path.display()))?;
    let decrypted = info.decrypt(pass).map_err(|_| {
        anyhow!(
            "Failed to decrypt private key '{}' (wrong --pass?)",
            path.display()
        )
    })?;
    Ok(PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(decrypted.as_bytes().to_vec()),
    ))
}

#[cfg(not(feature = "encrypted-keys"))]
fn decrypt_pkcs8(path: &Path, _text: &str, _pass: &str) -> Result<PrivateKeyDer<'static>> {
    bail!(
        "Private key '{}' is encrypted, but this build has no `encrypted-keys` feature; decrypt it with `openssl pkcs8 -in {} -out key-plain.pem` or use a default build",
        path.display(),
        path.display()
    )
}

pub fn load_key(path: &Path, pass: Option<&str>) -> Result<PrivateKeyDer<'static>> {
    let pem = read(path, "private key file")?;
    let text = String::from_utf8_lossy(&pem);
    if text.contains("BEGIN ENCRYPTED PRIVATE KEY") {
        let pass = pass.ok_or_else(|| {
            anyhow!(
                "Private key '{}' is encrypted; supply its passphrase with --pass",
                path.display()
            )
        })?;
        return decrypt_pkcs8(path, &text, pass);
    }
    if text.contains("Proc-Type: 4,ENCRYPTED") {
        bail!(
            "Legacy encrypted PEM key '{}' is not supported; convert it with `openssl pkcs8 -topk8 -in {} -out key-pkcs8.pem`",
            path.display(),
            path.display()
        );
    }
    match PrivateKeyDer::from_pem_slice(&pem) {
        Ok(key) => Ok(key),
        Err(rustls::pki_types::pem::Error::NoItemsFound) => {
            bail!(
                "No private key found in '{}'{}",
                path.display(),
                not_pem_hint(&pem, path, "pkey")
            )
        }
        Err(e) => {
            Err(anyhow!(e).context(format!("Failed to parse private key '{}'", path.display())))
        }
    }
}

/// Accept-anything verifier for `--insecure`. Signatures are still checked so
/// the handshake itself stays well-formed; only the chain/name are trusted.
#[derive(Debug)]
struct NoVerification(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Throwaway encrypted PKCS#8 key made with the system `openssl` (no key is
    /// committed: gitleaks would rightly flag it). None when openssl is absent.
    fn encrypted_key(dir: &Path) -> Option<std::path::PathBuf> {
        let plain = dir.join("plain.pem");
        let enc = dir.join("enc.pem");
        let ok = |args: &[&str]| {
            std::process::Command::new("openssl")
                .args(args)
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        let (p, e) = (plain.to_str()?, enc.to_str()?);
        (ok(&[
            "genpkey",
            "-algorithm",
            "EC",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-out",
            p,
        ]) && ok(&[
            "pkcs8",
            "-topk8",
            "-v2",
            "aes-256-cbc",
            "-in",
            p,
            "-out",
            e,
            "-passout",
            "pass:t3st",
        ]))
        .then_some(enc)
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("edrs-tls-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn encrypted_pkcs8_key_needs_pass() {
        let d = tmpdir("nopass");
        let Some(k) = encrypted_key(&d) else { return };
        let e = load_key(&k, None).unwrap_err().to_string();
        assert!(e.contains("--pass"), "{e}");
        std::fs::remove_dir_all(d).ok();
    }

    #[cfg(feature = "encrypted-keys")]
    #[test]
    fn encrypted_pkcs8_key_decrypts_with_pass() {
        let d = tmpdir("dec");
        let Some(k) = encrypted_key(&d) else { return };
        assert!(matches!(
            load_key(&k, Some("t3st")).unwrap(),
            PrivateKeyDer::Pkcs8(_)
        ));
        let e = load_key(&k, Some("wrong")).unwrap_err().to_string();
        assert!(e.contains("wrong --pass"), "{e}");
        std::fs::remove_dir_all(d).ok();
    }

    /// L4-004: pkcs8 0.11 still decrypts the PBES2 variants openssl writes: scrypt and
    /// PBKDF2-HMAC-SHA256, with AES-128/256-CBC. (The PBKDF2-HMAC-SHA1 PRF was never
    /// supported: pkcs5's `sha1-insecure` feature is off, in 0.7 as in 0.8.)
    #[cfg(feature = "encrypted-keys")]
    #[test]
    fn encrypted_pkcs8_variants_decrypt_after_the_pkcs8_0_11_bump() {
        let d = tmpdir("variants");
        let Some(base) = encrypted_key(&d) else {
            return;
        };
        let plain = base.with_file_name("plain.pem");
        let variants: [&[&str]; 2] = [
            &["-v2", "aes-128-cbc", "-scrypt"],
            &["-v2", "aes-128-cbc", "-v2prf", "hmacWithSHA256"],
        ];
        for (i, v) in variants.iter().enumerate() {
            let out = d.join(format!("v{i}.pem"));
            let mut args = vec!["pkcs8", "-topk8", "-in", plain.to_str().unwrap()];
            args.extend_from_slice(v);
            args.extend_from_slice(&["-out", out.to_str().unwrap(), "-passout", "pass:t3st"]);
            let made = std::process::Command::new("openssl")
                .args(&args)
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if !made {
                continue; // an openssl without this option
            }
            assert!(
                matches!(load_key(&out, Some("t3st")), Ok(PrivateKeyDer::Pkcs8(_))),
                "variant {v:?}"
            );
            assert!(load_key(&out, Some("nope")).is_err(), "variant {v:?}");
        }
        std::fs::remove_dir_all(d).ok();
    }

    #[cfg(not(feature = "encrypted-keys"))]
    #[test]
    fn encrypted_pkcs8_key_without_feature_is_a_clear_error() {
        let d = tmpdir("nofeat");
        let Some(k) = encrypted_key(&d) else { return };
        let e = load_key(&k, Some("t3st")).unwrap_err().to_string();
        assert!(
            e.contains("encrypted-keys") && e.contains("openssl pkcs8"),
            "{e}"
        );
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn default_config_builds_with_system_or_bundled_roots() {
        build_client_config(&TlsOptions::default()).unwrap();
    }

    #[test]
    fn insecure_config_builds() {
        build_client_config(&TlsOptions {
            insecure: true,
            ..Default::default()
        })
        .unwrap();
    }

    #[test]
    fn missing_ca_file_is_a_clear_error() {
        let err = build_client_config(&TlsOptions {
            ca: Some("/definitely/missing.pem".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(format!("{err:#}").contains("missing.pem"));
    }

    #[test]
    fn cert_without_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.pem");
        std::fs::write(&p, "x").unwrap();
        let err = build_client_config(&TlsOptions {
            cert: Some(p),
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.to_string().contains("--key"));
    }
}
