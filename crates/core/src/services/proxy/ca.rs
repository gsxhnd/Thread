//! Local certificate authority used for HTTPS interception.
//!
//! The root CA is created on first use and stored under `<数据目录>/proxy/ca/`:
//! `ca.crt.pem` (public), `ca.key.pem` (private, mode 0600) and `ca.json` (metadata).
//! Leaf certificates are issued per CONNECT host and cached in memory only.
//!
//! Trusting the root is a user decision made outside the app (see the certificate page).
//! This module never installs anything into the system trust store.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Datelike, Days, Months, NaiveDate, Utc};
use parking_lot::Mutex;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, date_time_ymd,
};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::atomic_file::atomic_write;
use crate::error::{AppError, AppResult};

pub const CA_COMMON_NAME: &str = "Thread Local CA";
const CA_ORGANIZATION: &str = "Thread";
const CA_CERT_FILE: &str = "ca.crt.pem";
const CA_KEY_FILE: &str = "ca.key.pem";
const CA_META_FILE: &str = "ca.json";
const CA_META_VERSION: u32 = 1;
const LEAF_CACHE_LIMIT: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaMeta {
    version: u32,
    common_name: String,
    created_at: i64,
}

/// Public facts about the root CA. The private key is never part of this type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaInfo {
    pub common_name: String,
    pub cert_pem: String,
    pub cert_path: String,
    pub fingerprint_sha256: String,
    pub created_at: i64,
}

struct CaMaterial {
    cert_pem: String,
    der: CertificateDer<'static>,
    created_at: i64,
    issuer: Issuer<'static, KeyPair>,
}

pub struct CaManager {
    dir: PathBuf,
    material: Mutex<Option<Arc<CaMaterial>>>,
    leaves: Mutex<HashMap<String, Arc<ServerConfig>>>,
}

impl CaManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            material: Mutex::new(None),
            leaves: Mutex::new(HashMap::new()),
        }
    }

    pub fn cert_path(&self) -> PathBuf {
        self.dir.join(CA_CERT_FILE)
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join(CA_KEY_FILE)
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join(CA_META_FILE)
    }

    pub fn exists(&self) -> bool {
        self.cert_path().is_file() && self.key_path().is_file()
    }

    /// Returns the current CA, creating it on first use.
    pub fn info(&self) -> AppResult<CaInfo> {
        let material = self.load_or_create()?;
        Ok(info_of(&material, &self.cert_path()))
    }

    /// Replaces the CA. Any client that trusted the previous root must trust the new one.
    pub fn regenerate(&self) -> AppResult<CaInfo> {
        let material = self.write_new()?;
        self.leaves.lock().clear();
        Ok(info_of(&material, &self.cert_path()))
    }

    /// Copies the public certificate (PEM) to a user-chosen path. The key is not exported.
    pub fn export_to(&self, target: &Path) -> AppResult<PathBuf> {
        let material = self.load_or_create()?;
        atomic_write(target, &material.cert_pem).map_err(AppError::Io)?;
        Ok(target.to_path_buf())
    }

    /// TLS server configuration presenting a certificate for `host`, signed by the root.
    pub fn server_config(&self, host: &str) -> AppResult<Arc<ServerConfig>> {
        let key = host.to_ascii_lowercase();
        if let Some(config) = self.leaves.lock().get(&key) {
            return Ok(Arc::clone(config));
        }
        let material = self.load_or_create()?;
        let config = Arc::new(issue_leaf(&material, &key)?);

        let mut leaves = self.leaves.lock();
        if leaves.len() >= LEAF_CACHE_LIMIT {
            leaves.clear();
        }
        leaves.insert(key, Arc::clone(&config));
        Ok(config)
    }

    fn load_or_create(&self) -> AppResult<Arc<CaMaterial>> {
        let mut slot = self.material.lock();
        if let Some(material) = slot.as_ref() {
            return Ok(Arc::clone(material));
        }
        let material = if self.exists() {
            Arc::new(self.load()?)
        } else {
            Arc::new(self.write_new()?)
        };
        *slot = Some(Arc::clone(&material));
        Ok(material)
    }

    fn load(&self) -> AppResult<CaMaterial> {
        let cert_pem = std::fs::read_to_string(self.cert_path())?;
        let key_pem = std::fs::read_to_string(self.key_path())?;
        let key = KeyPair::from_pem(&key_pem).map_err(tls_error)?;
        let issuer = Issuer::from_ca_cert_pem(&cert_pem, key).map_err(tls_error)?;
        let der = pem_to_der(&cert_pem)?;
        let created_at = match std::fs::read_to_string(self.meta_path())
            .ok()
            .and_then(|text| serde_json::from_str::<CaMeta>(&text).ok())
        {
            Some(meta) => meta.created_at,
            None => 0,
        };
        Ok(CaMaterial {
            cert_pem,
            der,
            created_at,
            issuer,
        })
    }

    fn write_new(&self) -> AppResult<CaMaterial> {
        let now = Utc::now();
        let key = KeyPair::generate().map_err(tls_error)?;

        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(tls_error)?;
        let mut name = DistinguishedName::new();
        name.push(DnType::CommonName, CA_COMMON_NAME);
        name.push(DnType::OrganizationName, CA_ORGANIZATION);
        params.distinguished_name = name;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let today = now.date_naive();
        let (y, m, d) = ymd_parts(today.checked_sub_days(Days::new(1)).unwrap_or(today));
        params.not_before = date_time_ymd(y, m, d);
        let (y, m, d) = ymd_parts(today.checked_add_months(Months::new(120)).unwrap_or(today));
        params.not_after = date_time_ymd(y, m, d);

        let certificate = params.self_signed(&key).map_err(tls_error)?;
        let cert_pem = certificate.pem();
        let meta = CaMeta {
            version: CA_META_VERSION,
            common_name: CA_COMMON_NAME.to_string(),
            created_at: now.timestamp_millis(),
        };

        atomic_write(&self.cert_path(), &cert_pem).map_err(AppError::Io)?;
        atomic_write(&self.key_path(), &key.serialize_pem()).map_err(AppError::Io)?;
        restrict_key_permissions(&self.key_path())?;
        atomic_write(
            &self.meta_path(),
            &(serde_json::to_string_pretty(&meta)? + "\n"),
        )
        .map_err(AppError::Io)?;

        // Round-trip through the stored PEM so the in-memory issuer matches what is on disk.
        self.load()
    }
}

fn info_of(material: &CaMaterial, cert_path: &Path) -> CaInfo {
    CaInfo {
        common_name: CA_COMMON_NAME.to_string(),
        cert_pem: material.cert_pem.clone(),
        cert_path: cert_path.to_string_lossy().into_owned(),
        fingerprint_sha256: fingerprint(&material.der),
        created_at: material.created_at,
    }
}

fn issue_leaf(material: &CaMaterial, host: &str) -> AppResult<ServerConfig> {
    let mut params = CertificateParams::new(vec![host.to_string()])
        .map_err(|error| AppError::Validation(format!("invalid host for certificate: {error}")))?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, host);
    params.distinguished_name = name;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let today = Utc::now().date_naive();
    let (y, m, d) = ymd_parts(today.checked_sub_days(Days::new(1)).unwrap_or(today));
    params.not_before = date_time_ymd(y, m, d);
    let (y, m, d) = ymd_parts(today.checked_add_days(Days::new(365)).unwrap_or(today));
    params.not_after = date_time_ymd(y, m, d);

    let key = KeyPair::generate().map_err(tls_error)?;
    let certificate = params
        .signed_by(&key, &material.issuer)
        .map_err(tls_error)?;
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));

    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(tls_error)?
        .with_no_client_auth()
        .with_single_cert(vec![certificate.der().clone()], key_der)
        .map_err(tls_error)?;
    // Captured traffic is parsed as HTTP/1.1 only.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn ymd_parts(date: NaiveDate) -> (i32, u8, u8) {
    (date.year(), date.month() as u8, date.day() as u8)
}

fn fingerprint(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    digest
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn pem_to_der(pem: &str) -> AppResult<CertificateDer<'static>> {
    let parsed =
        pem::parse(pem).map_err(|error| AppError::Tls(format!("invalid CA pem: {error}")))?;
    Ok(CertificateDer::from(parsed.into_contents()))
}

fn tls_error(error: impl std::fmt::Display) -> AppError {
    AppError::Tls(error.to_string())
}

#[cfg(unix)]
fn restrict_key_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_key_permissions(_path: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "thread-ca-{tag}-{}-{}",
            std::process::id(),
            crate::db::new_id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn creates_ca_once_and_reloads_the_same_root() {
        let dir = temp_dir("reload");
        let first = CaManager::new(dir.clone());
        let created = first.info().unwrap();
        assert!(created.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(created.fingerprint_sha256.contains(':'));

        // A fresh manager (simulating an app restart) must reuse the stored root.
        let second = CaManager::new(dir.clone());
        let reloaded = second.info().unwrap();
        assert_eq!(created.fingerprint_sha256, reloaded.fingerprint_sha256);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn regenerate_changes_the_root() {
        let dir = temp_dir("regen");
        let manager = CaManager::new(dir.clone());
        let before = manager.info().unwrap();
        let after = manager.regenerate().unwrap();
        assert_ne!(before.fingerprint_sha256, after.fingerprint_sha256);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn issues_leaf_configs_and_caches_them_per_host() {
        let dir = temp_dir("leaf");
        let manager = CaManager::new(dir.clone());
        let first = manager.server_config("Example.test").unwrap();
        let again = manager.server_config("example.test").unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(first.alpn_protocols, vec![b"http/1.1".to_vec()]);

        let ip = manager.server_config("127.0.0.1");
        assert!(ip.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn private_key_is_not_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perm");
        let manager = CaManager::new(dir.clone());
        manager.info().unwrap();
        let mode = std::fs::metadata(manager.key_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
