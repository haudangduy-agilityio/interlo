//! model-store: local model files, their pinned checksums and size caps.
//!
//! Owns: the compiled-in model table (`manifest.rs`: models, packs, files with
//! url, pinned SHA-256 and exact size, pinned hosts), the model spec (id, url,
//! pinned SHA-256, size cap), the download gate
//! (owner confirmation + https + the policy's model-host allow list, re-checked
//! on every redirect hop) and file verification. The hash function is passed in
//! through the `Hasher` trait.
//!
//! Must never: run a model that failed its checksum. Only `verify` can build a
//! `Verified` value, and it fails on a size over the cap or a checksum mismatch.
//! `Verified` holds the open file that was hashed; the loader reads that handle
//! and never opens the path again, so a swap after the check cannot reach it.
//! The file length comes from the same open file that is hashed, never from the
//! caller. A download starts only with an `OwnerConfirmation` (ADR 0006), and
//! only for a model found by id in the compiled-in table (`entry_for`): the
//! webview never supplies a URL.

use meeting_core::egress::HttpFetch;
use meeting_core::policy::{EgressDenied, PrivacyPolicy, Provider};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use thiserror::Error;

mod manifest;

pub use manifest::{ModelEntry, ModelFile, Pack};

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("download needs the owner's confirmation")]
    NotClicked,
    #[error("the owner confirmed model {confirmed}, not {wanted}")]
    ConfirmationForOtherModel { confirmed: String, wanted: String },
    #[error("model url must use https: {0}")]
    InsecureUrl(String),
    #[error("model url has no host: {0}")]
    NoHost(String),
    #[error(transparent)]
    Egress(#[from] EgressDenied),
    #[error("pinned checksum is not 64 hex characters")]
    BadChecksumPin,
    #[error("file is {len} bytes, the cap is {cap} bytes")]
    SizeOverCap { len: u64, cap: u64 },
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("the download was cancelled")]
    Cancelled,
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: String,
    pub url: String,
    /// Pinned SHA-256 of the file, 64 hex characters.
    pub sha256_hex: String,
    pub size_cap_bytes: u64,
}

/// What a `Hasher` found in one open file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDigest {
    /// The open file is longer than the cap. Its content was not read.
    OverCap { len: u64 },
    /// The SHA-256 of the whole content that was read, and its length in bytes.
    Hashed { len: u64, sha256_hex: String },
}

/// Computes the SHA-256 of an open model file. Implemented by the host app;
/// tests use a fake.
pub trait Hasher {
    /// `file` is the one handle that `verify` opened and keeps. Take the length
    /// from that handle (not from a `stat` on a path, which a swapped file could
    /// answer differently). When the length is over `cap_bytes`, return
    /// `FileDigest::OverCap` without reading the content. Otherwise hash that
    /// same handle from the start and return the number of bytes hashed with
    /// the digest. Never read more than `cap_bytes` bytes. Do not open a path.
    fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest>;
}

/// Proof that a file passed `verify`, and the open file that was checked. It
/// cannot be built any other way.
///
/// Load the model from `into_file`. Do not open the original path again: the
/// file there can be replaced after the check.
#[derive(Debug)]
pub struct Verified {
    id: String,
    file: File,
    sha256_hex: String,
    size_bytes: u64,
}

impl Verified {
    pub fn id(&self) -> &str {
        &self.id
    }
    /// The checked file, open and rewound to its start.
    pub fn into_file(self) -> File {
        self.file
    }
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }
    /// The length of the file that was hashed.
    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

/// Proof that the owner confirmed the download of one model in the shell's
/// native dialog (ADR 0012). No webview value can make one: it has no public
/// field, no `Default`, no `Clone` and no `Deserialize`.
#[derive(Debug, PartialEq, Eq)]
pub struct OwnerConfirmation {
    model_id: String,
}

impl OwnerConfirmation {
    /// The only constructor. Call it only from the shell's native dialog module
    /// (`apps/desktop/src-tauri/src/native_confirm.rs`), and only with the result
    /// of the owner's click on the confirm button. The shell's tests fail when
    /// any other file calls it.
    pub fn from_native_dialog(model_id: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
        }
    }

    /// The model the owner confirmed.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Check a downloaded file. `verify` opens `path` once and keeps that handle in
/// the result. The hasher reads the length from the open file and stops before
/// hashing when it is over the size cap; `verify` also refuses a hasher that
/// reports a length over the cap, so no caller can supply a length. The
/// checksum compare ignores hex case.
///
/// The handle protects against a path that is swapped after the check. It does
/// not protect against a process that writes into the same file in place.
pub fn verify(spec: &ModelSpec, hasher: &dyn Hasher, path: &Path) -> Result<Verified, ModelError> {
    if !is_sha256_hex(&spec.sha256_hex) {
        return Err(ModelError::BadChecksumPin);
    }
    let cap = spec.size_cap_bytes;
    let mut file = File::open(path)?;
    let (len, actual) = match hasher.digest(&mut file, cap)? {
        FileDigest::OverCap { len } => return Err(ModelError::SizeOverCap { len, cap }),
        FileDigest::Hashed { len, sha256_hex } => (len, sha256_hex.to_ascii_lowercase()),
    };
    if len > cap {
        return Err(ModelError::SizeOverCap { len, cap });
    }
    let expected = spec.sha256_hex.to_ascii_lowercase();
    if actual != expected {
        return Err(ModelError::ChecksumMismatch { expected, actual });
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(Verified {
        id: spec.id.clone(),
        file,
        sha256_hex: actual,
        size_bytes: len,
    })
}

/// The host of an https url, as the policy's allow list names it.
fn https_host(url: &str) -> Result<String, ModelError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| ModelError::InsecureUrl(url.to_string()))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err(ModelError::NoHost(url.to_string()));
    }
    Ok(host.to_string())
}

/// The remote host that serves a model, as an egress-guard provider.
pub fn host_provider(spec: &ModelSpec) -> Result<Provider, ModelError> {
    Ok(Provider::remote(https_host(&spec.url)?))
}

/// A download may start only when the owner confirmed this model in the native
/// dialog (`confirmation` is `Some` and names `spec.id`), the url is https, and
/// the policy's model-host allow list has this host. The request carries no
/// user data, so it never asks the policy for a data-kind consent.
///
/// This checks the first url only. The downloader must call `check_redirect`
/// for every redirect hop before it follows it.
pub fn may_download(
    confirmation: Option<&OwnerConfirmation>,
    policy: &PrivacyPolicy,
    spec: &ModelSpec,
) -> Result<(), ModelError> {
    let confirmation = confirmation.ok_or(ModelError::NotClicked)?;
    if confirmation.model_id() != spec.id {
        return Err(ModelError::ConfirmationForOtherModel {
            confirmed: confirmation.model_id().to_string(),
            wanted: spec.id.clone(),
        });
    }
    let host = host_provider(spec)?;
    policy.check_model_download(host.id())?;
    Ok(())
}

/// Check one redirect hop of a download: the same https and allow-list rule as
/// `may_download`, applied to `next_url`. A server on an allowed host can send
/// the client anywhere, so the downloader must call this for every hop (and
/// stop after a small fixed number of hops) before it follows a `Location`.
/// `from_spec` names the download this hop belongs to; the verdict depends only
/// on `next_url` and the policy.
pub fn check_redirect(
    policy: &PrivacyPolicy,
    _from_spec: &ModelSpec,
    next_url: &str,
) -> Result<(), ModelError> {
    let host = https_host(next_url)?;
    policy.check_model_download(&host)?;
    Ok(())
}

/// The models of the compiled-in table, in Setup order. The webview and the
/// settings never supply a url: this table is the only source (ADR 0012).
pub fn entries() -> &'static [ModelEntry] {
    manifest::ENTRIES
}

/// The packs of the compiled-in table. The downloader (M1-04.4) walks them.
pub fn packs() -> &'static [Pack] {
    manifest::PACKS
}

/// The hosts a download may contact (`PINNED_HOSTS`). The downloader (M1-04.4)
/// checks every redirect hop against them.
pub fn pinned_hosts() -> &'static [&'static str] {
    manifest::PINNED_HOSTS
}

/// Resolve `id` in `table`: an exact id match, never read as a url.
pub fn entry_in<'a>(table: &'a [ModelEntry], id: &str) -> Option<&'a ModelEntry> {
    table.iter().find(|m| m.id == id)
}

/// Resolve a model by id in the compiled-in table. Only an exact id matches: an
/// unknown id, a different case or a url returns `None`. The id is never read as
/// a url.
pub fn entry_for(id: &str) -> Option<&'static ModelEntry> {
    entry_in(entries(), id)
}

/// The pack that holds the files of `entry`. The test
/// `every_entry_names_an_existing_pack` guarantees that it exists, so a missing
/// pack is a build error of the table, not a runtime case.
#[allow(clippy::expect_used)] // invariant pinned by the table test named above
pub fn pack_of(entry: &ModelEntry) -> &'static Pack {
    packs()
        .iter()
        .find(|p| p.id == entry.pack)
        .expect("every entry names a pack of the table")
}

/// The hosts a pack's files come from, each once, in file order. The native
/// dialog names them so the owner sees where the bytes come from.
pub fn pack_hosts(pack: &Pack) -> Vec<String> {
    let mut hosts: Vec<String> = Vec::new();
    for file in pack.files {
        if let Ok(host) = https_host(file.url) {
            if !hosts.contains(&host) {
                hosts.push(host);
            }
        }
    }
    hosts
}

/// The total download size of a pack: the sum of its files' table sizes.
pub fn pack_size_bytes(pack: &Pack) -> u64 {
    pack.files.iter().map(|f| f.size_bytes).sum()
}

/// A cancel flag shared by the downloader and the cancel command. A clone sees
/// the same flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A download of this pack is already running.
#[derive(Debug, PartialEq, Eq, Error)]
#[error("a download of this pack is already running")]
pub struct AlreadyRunning;

/// The downloads that run now, one per pack. The lock is held only to read or
/// change the map, never while a dialog is open or a file is transferred, so a
/// cancel can always get in.
#[derive(Debug, Default)]
pub struct DownloadRegistry {
    running: Mutex<HashMap<&'static str, CancelToken>>,
}

impl DownloadRegistry {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<&'static str, CancelToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Start a download of `pack_id`. The returned guard frees the pack when it
    /// is dropped, whatever way the download ends.
    pub fn begin(&self, pack_id: &'static str) -> Result<DownloadGuard<'_>, AlreadyRunning> {
        let mut running = self.map();
        if running.contains_key(pack_id) {
            return Err(AlreadyRunning);
        }
        let token = CancelToken::default();
        running.insert(pack_id, token.clone());
        Ok(DownloadGuard {
            registry: self,
            pack_id,
            token,
        })
    }

    pub fn is_running(&self, pack_id: &str) -> bool {
        self.map().contains_key(pack_id)
    }

    /// Ask the running download of `pack_id` to stop. `false` when none runs.
    pub fn cancel(&self, pack_id: &str) -> bool {
        match self.map().get(pack_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}

/// Proof that a pack's download is registered. Dropping it frees the pack.
#[derive(Debug)]
pub struct DownloadGuard<'a> {
    registry: &'a DownloadRegistry,
    pack_id: &'static str,
    token: CancelToken,
}

impl DownloadGuard<'_> {
    pub fn token(&self) -> &CancelToken {
        &self.token
    }
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.registry.map().remove(self.pack_id);
    }
}

/// Remove every file whose name ends in `.part` under `dir`. A cancelled or
/// failed download leaves no partial file (A-2). A folder that does not exist is
/// fine; finished files stay.
pub fn remove_partial_files(dir: &Path) -> io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        // `file_type` does not follow a symlink, so a link is never walked into.
        if entry.file_type()?.is_dir() {
            remove_partial_files(&path)?;
        } else if path.extension().is_some_and(|ext| ext == "part") {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Fetch a model to `dest`. Stub: the transfer is M1-04.4. The gate runs first,
/// so a missing or wrong confirmation is refused before anything else. The stub
/// makes no fetch call.
pub fn download(
    spec: &ModelSpec,
    confirmation: Option<OwnerConfirmation>,
    policy: &PrivacyPolicy,
    _fetch: &dyn HttpFetch,
    _dest: &Path,
) -> Result<(), ModelError> {
    may_download(confirmation.as_ref(), policy, spec)?;
    Err(ModelError::NotImplemented("download"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use meeting_core::egress::{FetchError, FetchRequest, FetchResponse, HttpFetch};
    use meeting_core::protocol::DataKind;
    use std::cell::Cell;

    const GOOD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// A file of `len` bytes. It honours the `Hasher` contract: it does not
    /// read (count) a file that is over the cap.
    struct FakeHasher {
        len: u64,
        digest: String,
        hashed: Cell<u32>,
    }

    impl FakeHasher {
        fn new(digest: &str) -> Self {
            Self::with_len(digest, 10)
        }

        fn with_len(digest: &str, len: u64) -> Self {
            Self {
                len,
                digest: digest.into(),
                hashed: Cell::new(0),
            }
        }
    }

    impl Hasher for FakeHasher {
        fn digest(&self, _file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
            if self.len > cap_bytes {
                return Ok(FileDigest::OverCap { len: self.len });
            }
            self.hashed.set(self.hashed.get() + 1);
            Ok(FileDigest::Hashed {
                len: self.len,
                sha256_hex: self.digest.clone(),
            })
        }
    }

    /// A broken hasher that hashes whatever it is given and ignores the cap.
    struct IgnoresCap(u64);

    impl Hasher for IgnoresCap {
        fn digest(&self, _file: &mut File, _cap_bytes: u64) -> io::Result<FileDigest> {
            Ok(FileDigest::Hashed {
                len: self.0,
                sha256_hex: GOOD.into(),
            })
        }
    }

    /// A small file in the temp dir that is removed on drop.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(content: &[u8]) -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("model-store-test-{}-{n}.bin", std::process::id()));
            std::fs::write(&path, content).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// The owner's confirmation for the model that `spec()` describes.
    fn click() -> OwnerConfirmation {
        OwnerConfirmation::from_native_dialog("small-model")
    }

    /// Hex of the file content, read from the handle. With a 32-byte file it is
    /// a 64-character pin, so a test can tell which bytes `verify` hashed.
    struct HexOfContent;

    impl Hasher for HexOfContent {
        fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
            use std::io::Read;
            let len = file.metadata()?.len();
            if len > cap_bytes {
                return Ok(FileDigest::OverCap { len });
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            let sha256_hex = bytes.iter().map(|b| format!("{b:02x}")).collect();
            Ok(FileDigest::Hashed {
                len: bytes.len() as u64,
                sha256_hex,
            })
        }
    }

    fn spec() -> ModelSpec {
        ModelSpec {
            id: "small-model".into(),
            url: "https://models.example.com/files/small.bin".into(),
            sha256_hex: GOOD.into(),
            size_cap_bytes: 1_000,
        }
    }

    #[test]
    fn matching_file_is_verified() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::with_len(GOOD, 999);
        let v = verify(&spec(), &h, f.path()).unwrap();
        assert_eq!(v.id(), "small-model");
        assert_eq!(v.sha256_hex(), GOOD);
        assert_eq!(v.size_bytes(), 999);
    }

    #[test]
    fn file_at_the_cap_is_allowed() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::with_len(GOOD, 1_000);
        let v = verify(&spec(), &h, f.path()).unwrap();
        assert_eq!(v.size_bytes(), 1_000);
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::new(OTHER);
        let err = verify(&spec(), &h, f.path()).unwrap_err();
        assert!(matches!(err, ModelError::ChecksumMismatch { .. }));
    }

    #[test]
    fn size_over_cap_is_rejected_before_hashing() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::with_len(GOOD, 1_001);
        let err = verify(&spec(), &h, f.path()).unwrap_err();
        assert!(matches!(
            err,
            ModelError::SizeOverCap {
                len: 1_001,
                cap: 1_000
            }
        ));
        assert_eq!(h.hashed.get(), 0, "an oversized file must not be hashed");
    }

    #[test]
    fn the_length_comes_from_the_file_not_the_caller() {
        let f = TempFile::new(b"model");
        // The old API took the length as a parameter, so a caller could pass 0
        // for a 5000-byte file. `verify` has no length parameter any more.
        let h = FakeHasher::with_len(GOOD, 5_000);
        assert!(matches!(
            verify(&spec(), &h, f.path()),
            Err(ModelError::SizeOverCap {
                len: 5_000,
                cap: 1_000
            })
        ));
        assert_eq!(h.hashed.get(), 0);
    }

    #[test]
    fn an_empty_file_has_length_zero_and_must_still_match_the_pin() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::with_len(OTHER, 0);
        assert!(matches!(
            verify(&spec(), &h, f.path()),
            Err(ModelError::ChecksumMismatch { .. })
        ));
        let h = FakeHasher::with_len(GOOD, 0);
        assert_eq!(verify(&spec(), &h, f.path()).unwrap().size_bytes(), 0);
    }

    #[test]
    fn a_hasher_that_ignores_the_cap_cannot_pass_an_oversized_file() {
        let f = TempFile::new(b"model");
        let err = verify(&spec(), &IgnoresCap(1_001), f.path()).unwrap_err();
        assert!(matches!(
            err,
            ModelError::SizeOverCap {
                len: 1_001,
                cap: 1_000
            }
        ));
    }

    #[test]
    fn checksum_compare_ignores_case() {
        let f = TempFile::new(b"model");
        let h = FakeHasher::new(&GOOD.to_uppercase());
        let v = verify(&spec(), &h, f.path()).unwrap();
        assert_eq!(v.sha256_hex(), GOOD, "the stored digest is lower case");
    }

    #[test]
    fn a_bad_pin_is_rejected() {
        let f = TempFile::new(b"model");
        let mut s = spec();
        s.sha256_hex = "abc".into();
        let h = FakeHasher::new("abc");
        assert!(matches!(
            verify(&s, &h, f.path()),
            Err(ModelError::BadChecksumPin)
        ));
        s.sha256_hex = "z".repeat(64);
        assert!(matches!(
            verify(&s, &h, f.path()),
            Err(ModelError::BadChecksumPin)
        ));
        assert_eq!(h.hashed.get(), 0, "a bad pin is refused before any read");
    }

    #[test]
    fn hasher_io_error_is_not_a_pass() {
        let f = TempFile::new(b"model");
        struct Broken;
        impl Hasher for Broken {
            fn digest(&self, _file: &mut File, _cap: u64) -> io::Result<FileDigest> {
                Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
            }
        }
        assert!(matches!(
            verify(&spec(), &Broken, f.path()),
            Err(ModelError::Io(_))
        ));
    }

    fn consenting_policy() -> PrivacyPolicy {
        let mut p = PrivacyPolicy::local_only();
        p.set_local_only(false);
        p.allow_model_host("models.example.com").unwrap();
        p
    }

    fn local_only_policy() -> PrivacyPolicy {
        let mut p = PrivacyPolicy::local_only();
        p.allow_model_host("models.example.com").unwrap();
        p
    }

    #[test]
    fn download_without_a_click_is_refused() {
        // Even when the policy would allow the host.
        assert!(matches!(
            may_download(None, &consenting_policy(), &spec()),
            Err(ModelError::NotClicked)
        ));
    }

    #[test]
    fn download_after_a_click_and_allowed_host_is_allowed() {
        assert!(may_download(Some(&click()), &consenting_policy(), &spec()).is_ok());
    }

    #[test]
    fn local_only_mode_allows_an_owner_started_download_from_an_allowed_host() {
        assert!(may_download(Some(&click()), &local_only_policy(), &spec()).is_ok());
    }

    #[test]
    fn no_click_is_refused_in_local_only_mode_too() {
        assert!(matches!(
            may_download(None, &local_only_policy(), &spec()),
            Err(ModelError::NotClicked)
        ));
    }

    #[test]
    fn host_not_on_the_list_is_refused_after_a_click() {
        let mut open = PrivacyPolicy::local_only();
        open.set_local_only(false);
        for policy in [PrivacyPolicy::local_only(), open] {
            assert!(matches!(
                may_download(Some(&click()), &policy, &spec()),
                Err(ModelError::Egress(EgressDenied::ModelHostNotAllowed(_)))
            ));
        }
        let mut other = spec();
        other.url = "https://elsewhere.example.org/small.bin".into();
        assert!(matches!(
            may_download(Some(&click()), &local_only_policy(), &other),
            Err(ModelError::Egress(EgressDenied::ModelHostNotAllowed(_)))
        ));
    }

    #[test]
    fn revoked_host_is_refused() {
        let mut p = local_only_policy();
        p.revoke_model_host("models.example.com");
        assert!(matches!(
            may_download(Some(&click()), &p, &spec()),
            Err(ModelError::Egress(EgressDenied::ModelHostNotAllowed(_)))
        ));
    }

    #[test]
    fn an_allowed_model_host_does_not_open_user_data() {
        let policy = local_only_policy();
        let host = host_provider(&spec()).unwrap();
        assert!(policy.check(&host, DataKind::Text).is_err());
        assert!(policy.check(&host, DataKind::Audio).is_err());
    }

    #[test]
    fn plain_http_is_refused() {
        let mut s = spec();
        s.url = "http://models.example.com/small.bin".into();
        assert!(matches!(
            may_download(Some(&click()), &consenting_policy(), &s),
            Err(ModelError::InsecureUrl(_))
        ));
    }

    #[test]
    fn host_is_read_from_the_url() {
        let mut s = spec();
        s.url = "https://models.example.com:8443?x=1".into();
        assert_eq!(host_provider(&s).unwrap().id(), "models.example.com:8443");
        s.url = "https:///path".into();
        assert!(matches!(host_provider(&s), Err(ModelError::NoHost(_))));
    }

    #[test]
    fn a_redirect_inside_the_allow_list_is_allowed() {
        assert!(check_redirect(
            &local_only_policy(),
            &spec(),
            "https://models.example.com/mirror/small.bin?token=1"
        )
        .is_ok());
    }

    #[test]
    fn a_redirect_from_an_allowed_host_to_a_disallowed_host_is_refused() {
        let policy = local_only_policy();
        // The first url passes the gate, the hop target does not.
        assert!(may_download(Some(&click()), &policy, &spec()).is_ok());
        assert!(matches!(
            check_redirect(&policy, &spec(), "https://cdn.elsewhere.example.org/small.bin"),
            Err(ModelError::Egress(EgressDenied::ModelHostNotAllowed(h)))
                if h == "cdn.elsewhere.example.org"
        ));
    }

    #[test]
    fn a_redirect_to_http_is_refused_even_on_an_allowed_host() {
        assert!(matches!(
            check_redirect(
                &consenting_policy(),
                &spec(),
                "http://models.example.com/small.bin"
            ),
            Err(ModelError::InsecureUrl(_))
        ));
    }

    #[test]
    fn a_redirect_cannot_hide_the_host_behind_user_info_or_a_missing_host() {
        let policy = local_only_policy();
        for next in [
            "https://models.example.com@evil.example.org/x.bin",
            "https://models.example.com.evil.example.org/x.bin",
            "ftp://models.example.com/x.bin",
            "//models.example.com/x.bin",
            "/relative/x.bin",
        ] {
            assert!(
                check_redirect(&policy, &spec(), next).is_err(),
                "{next} must be refused"
            );
        }
        assert!(matches!(
            check_redirect(&policy, &spec(), "https:///x.bin"),
            Err(ModelError::NoHost(_))
        ));
    }

    #[test]
    fn a_host_revoked_during_a_download_is_refused_on_the_next_hop() {
        let mut p = local_only_policy();
        assert!(check_redirect(&p, &spec(), "https://models.example.com/a").is_ok());
        p.revoke_model_host("models.example.com");
        assert!(check_redirect(&p, &spec(), "https://models.example.com/b").is_err());
    }

    fn model_entry(id: &'static str) -> ModelEntry {
        ModelEntry {
            id,
            name: "Test model",
            licence: "Test licence",
            non_commercial: false,
            pack: "test",
        }
    }

    #[test]
    fn lookup_matches_the_exact_id_only() {
        let table = [model_entry("small-model"), model_entry("large-model")];
        assert_eq!(entry_in(&table, "large-model").unwrap().id, "large-model");
        assert!(entry_in(&table, "Large-Model").is_none());
        assert!(entry_in(&table, "large").is_none());
        assert!(entry_in(&table, "https://models.example.com/files/small.bin").is_none());
    }

    /// A fetch that fails the test when it is used: the stub downloader must not
    /// reach for the network.
    struct NoFetch;

    impl HttpFetch for NoFetch {
        fn get(&self, _request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            panic!("the stub downloader must make no fetch call");
        }
    }

    /// A folder in the temp dir that is removed on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("model-store-dir-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_registry_allows_one_download_per_pack() {
        let registry = DownloadRegistry::default();
        let first = registry.begin("voice").unwrap();
        assert!(registry.is_running("voice"));
        assert!(registry.begin("voice").is_err(), "same pack twice");
        let other = registry.begin("asr").expect("another pack may run");
        assert!(registry.is_running("asr"));
        drop(first);
        assert!(
            !registry.is_running("voice"),
            "a finished download frees its pack"
        );
        assert!(registry.is_running("asr"), "the other pack still runs");
        registry.begin("voice").expect("the pack can start again");
        drop(other);
    }

    #[test]
    fn cancel_sets_the_token_of_the_running_pack_only() {
        let registry = DownloadRegistry::default();
        let voice = registry.begin("voice").unwrap();
        let asr = registry.begin("asr").unwrap();
        assert!(registry.cancel("voice"), "a running pack is cancelled");
        assert!(voice.token().is_cancelled());
        assert!(!asr.token().is_cancelled(), "no other pack is touched");
    }

    #[test]
    fn cancel_of_a_pack_with_no_download_changes_nothing() {
        let registry = DownloadRegistry::default();
        assert!(!registry.cancel("voice"));
        let guard = registry.begin("voice").unwrap();
        assert!(
            !guard.token().is_cancelled(),
            "an earlier cancel does not carry over to a later download"
        );
    }

    #[test]
    fn remove_partial_files_removes_part_files_and_keeps_the_rest() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.bin.part"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/b.bin.part"), b"x").unwrap();
        std::fs::write(dir.path().join("keep.bin"), b"x").unwrap();
        std::fs::write(dir.path().join("sub/part"), b"x").unwrap();
        remove_partial_files(dir.path()).unwrap();
        assert!(!dir.path().join("a.bin.part").exists());
        assert!(!dir.path().join("sub/b.bin.part").exists());
        assert!(
            dir.path().join("keep.bin").exists(),
            "a finished file stays"
        );
        assert!(
            dir.path().join("sub/part").exists(),
            "only a name that ends in .part goes"
        );
        remove_partial_files(&dir.path().join("missing")).expect("a missing folder is fine");
    }

    #[test]
    fn remove_partial_files_does_not_follow_a_symlink() {
        let dir = TempDir::new();
        let outside = TempDir::new();
        std::fs::write(outside.path().join("other.bin.part"), b"x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        remove_partial_files(dir.path()).unwrap();
        assert!(
            outside.path().join("other.bin.part").exists(),
            "a file behind a symlink is not ours to remove"
        );
    }

    #[test]
    fn a_pack_names_its_hosts_once_and_its_size_is_the_sum_of_its_files() {
        for pack in packs() {
            let mut expected: Vec<String> = Vec::new();
            for file in pack.files {
                let host = file
                    .url
                    .strip_prefix("https://")
                    .unwrap()
                    .split('/')
                    .next()
                    .unwrap()
                    .to_string();
                if !expected.contains(&host) {
                    expected.push(host);
                }
            }
            assert_eq!(pack_hosts(pack), expected, "pack {}", pack.id);
            assert_eq!(
                pack_size_bytes(pack),
                pack.files.iter().map(|f| f.size_bytes).sum::<u64>(),
                "pack {}",
                pack.id
            );
        }
    }

    #[test]
    fn download_is_a_stub() {
        assert!(matches!(
            download(
                &spec(),
                Some(click()),
                &consenting_policy(),
                &NoFetch,
                Path::new("m.bin")
            ),
            Err(ModelError::NotImplemented(_))
        ));
    }

    #[test]
    fn the_checked_file_is_still_the_checked_file_after_the_path_is_swapped() {
        use std::io::Read;
        let good = [b'A'; 32];
        let evil = [b'B'; 32];
        let pin: String = good.iter().map(|b| format!("{b:02x}")).collect();
        let s = ModelSpec {
            sha256_hex: pin,
            ..spec()
        };
        let f = TempFile::new(&good);
        let verified = verify(&s, &HexOfContent, f.path()).unwrap();
        // Replace the file at the same path: a new inode with other bytes.
        std::fs::remove_file(f.path()).unwrap();
        std::fs::write(f.path(), evil).unwrap();
        // The path now leads to the swapped bytes: reopening it would load them.
        assert_eq!(std::fs::read(f.path()).unwrap(), evil);
        // The handle in `Verified` still reads the bytes that were hashed.
        let mut loaded = Vec::new();
        verified.into_file().read_to_end(&mut loaded).unwrap();
        assert_eq!(loaded, good);
    }

    #[test]
    fn the_returned_file_is_rewound_to_its_start() {
        use std::io::Read;
        let good = [b'A'; 32];
        let pin: String = good.iter().map(|b| format!("{b:02x}")).collect();
        let s = ModelSpec {
            sha256_hex: pin,
            ..spec()
        };
        let f = TempFile::new(&good);
        let mut loaded = Vec::new();
        verify(&s, &HexOfContent, f.path())
            .unwrap()
            .into_file()
            .read_to_end(&mut loaded)
            .unwrap();
        assert_eq!(loaded, good);
    }

    #[test]
    fn a_missing_file_is_an_io_error_not_a_pass() {
        let f = TempFile::new(b"x");
        let gone = f.path().to_path_buf();
        drop(f);
        assert!(matches!(
            verify(&spec(), &FakeHasher::new(GOOD), &gone),
            Err(ModelError::Io(_))
        ));
    }

    #[test]
    fn a_confirmation_for_another_model_is_refused() {
        let other = OwnerConfirmation::from_native_dialog("large-model");
        assert_eq!(other.model_id(), "large-model");
        assert!(matches!(
            may_download(Some(&other), &consenting_policy(), &spec()),
            Err(ModelError::ConfirmationForOtherModel { confirmed, wanted })
                if confirmed == "large-model" && wanted == "small-model"
        ));
    }

    #[test]
    fn download_without_a_confirmation_is_refused_before_the_stub_answer() {
        assert!(matches!(
            download(
                &spec(),
                None,
                &consenting_policy(),
                &NoFetch,
                Path::new("m.bin")
            ),
            Err(ModelError::NotClicked)
        ));
    }

    #[test]
    fn download_with_a_confirmation_but_a_host_off_the_list_is_refused() {
        assert!(matches!(
            download(
                &spec(),
                Some(click()),
                &PrivacyPolicy::local_only(),
                &NoFetch,
                Path::new("m.bin")
            ),
            Err(ModelError::Egress(EgressDenied::ModelHostNotAllowed(_)))
        ));
    }
}
