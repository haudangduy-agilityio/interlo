//! Egress guard (handoff: "Privacy policy is enforced in host/backend, not only UI").
//!
//! Every outbound call goes through `check` first. Local providers always pass.
//! Remote providers need: local-only mode off, and an explicit consent record
//! for that provider AND that data kind (audio vs text).
//!
//! A model download is the one other network use. It sends no user data, so it
//! has its own path: `check_model_download` passes only for a host on the
//! owner's allow list. That list never grants a data kind to `check`. Only a
//! public DNS name can join the list: IP literals, `localhost` and private or
//! link-local names are refused, so a download cannot be aimed at the
//! owner's own network.

use crate::protocol::DataKind;
use std::collections::BTreeSet;
use thiserror::Error;

/// A party an outbound call goes to. Build it with `Provider::local` or
/// `Provider::remote` and read it with `id` and `is_remote`, so every call site
/// says on its own line that the provider skips or takes the egress check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    id: String,
    remote: bool,
}

impl Provider {
    /// A provider on this Mac. It always passes `PrivacyPolicy::check`.
    pub fn local(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            remote: false,
        }
    }

    /// A provider off this Mac. `PrivacyPolicy::check` needs local-only mode off
    /// and a consent record for it.
    pub fn remote(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            remote: true,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_remote(&self) -> bool {
        self.remote
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyPolicy {
    /// When true, no remote provider may be called, regardless of consent.
    /// Private on purpose: read it with `is_local_only`, change it with
    /// `set_local_only`.
    local_only: bool,
    /// Consent records: (provider id, data kind).
    consents: BTreeSet<(String, DataKind)>,
    /// Hosts the owner allows for model downloads (lowercase). Empty by default.
    /// A model request carries no user data, so this list never affects `check`.
    model_hosts: BTreeSet<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EgressDenied {
    #[error("local-only mode blocks remote provider {0}")]
    LocalOnly(String),
    #[error("no consent for provider {0} to send {1:?}")]
    NoConsent(String, DataKind),
    #[error("model host {0} is not on the allow list")]
    ModelHostNotAllowed(String),
}

/// Why `PrivacyPolicy::allow_model_host` refused a host name.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelHostRejected {
    #[error("model host is empty")]
    Empty,
    #[error("model host {0:?} is an IP address, not a public name")]
    IpLiteral(String),
    #[error("model host {0:?} points at this machine")]
    Localhost(String),
    #[error("model host {0:?} is a private or link-local name")]
    PrivateName(String),
    #[error("model host {0:?} is not a plain DNS name")]
    InvalidName(String),
}

/// Name suffixes that resolve inside a private network or on the local link.
const PRIVATE_SUFFIXES: [&str; 9] = [
    ".local",
    ".localdomain",
    ".lan",
    ".internal",
    ".intranet",
    ".corp",
    ".home",
    ".private",
    ".home.arpa",
];

/// Longest DNS name and label, in bytes (RFC 1035).
const MAX_HOST_BYTES: usize = 253;
const MAX_LABEL_BYTES: usize = 63;

impl Default for PrivacyPolicy {
    /// The privacy default is closed: same as `local_only()`.
    fn default() -> Self {
        Self::local_only()
    }
}

/// Validate a model host and return its lowercase form without a trailing dot.
fn normalize_model_host(host: &str) -> Result<String, ModelHostRejected> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(ModelHostRejected::Empty);
    }
    let bare = trimmed
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(trimmed);
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return Err(ModelHostRejected::IpLiteral(host.to_string()));
    }
    let name = bare.strip_suffix('.').unwrap_or(bare).to_ascii_lowercase();
    if name.is_empty() {
        return Err(ModelHostRejected::Empty);
    }
    // A name whose last label is a number is read as an IPv4 address by
    // resolvers ("2130706433", "0x7f.1"), so treat it as an IP literal.
    let last = name.rsplit('.').next().unwrap_or_default();
    let numeric_last = !last.is_empty()
        && (last.bytes().all(|b| b.is_ascii_digit())
            || last
                .strip_prefix("0x")
                .is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit())));
    if numeric_last {
        return Err(ModelHostRejected::IpLiteral(host.to_string()));
    }
    if name == "localhost" || name.ends_with(".localhost") {
        return Err(ModelHostRejected::Localhost(host.to_string()));
    }
    let label_ok = |l: &str| {
        !l.is_empty()
            && l.len() <= MAX_LABEL_BYTES
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if name.len() > MAX_HOST_BYTES || !name.split('.').all(label_ok) {
        return Err(ModelHostRejected::InvalidName(host.to_string()));
    }
    // One label is a search-domain name on the owner's network, never a
    // public host.
    if !name.contains('.') || PRIVATE_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        return Err(ModelHostRejected::PrivateName(host.to_string()));
    }
    Ok(name)
}

impl PrivacyPolicy {
    pub fn local_only() -> Self {
        Self {
            local_only: true,
            consents: BTreeSet::new(),
            model_hosts: BTreeSet::new(),
        }
    }

    pub fn is_local_only(&self) -> bool {
        self.local_only
    }

    pub fn set_local_only(&mut self, local_only: bool) {
        self.local_only = local_only;
    }

    pub fn grant(&mut self, provider_id: &str, kind: DataKind) {
        self.consents.insert((provider_id.to_string(), kind));
    }

    pub fn revoke(&mut self, provider_id: &str, kind: DataKind) {
        self.consents.remove(&(provider_id.to_string(), kind));
    }

    /// Allow model downloads from `host`. Host names compare in lowercase.
    /// Refuses an empty host, an IP literal, `localhost` and private or
    /// link-local names.
    pub fn allow_model_host(&mut self, host: &str) -> Result<(), ModelHostRejected> {
        self.model_hosts.insert(normalize_model_host(host)?);
        Ok(())
    }

    pub fn revoke_model_host(&mut self, host: &str) {
        self.model_hosts.remove(&host.to_ascii_lowercase());
    }

    /// Gate for a model download. It passes for an allowed host in any mode,
    /// including local-only, because the request carries no user data. It does
    /// not grant any `DataKind`.
    pub fn check_model_download(&self, host: &str) -> Result<(), EgressDenied> {
        if self.model_hosts.contains(&host.to_ascii_lowercase()) {
            Ok(())
        } else {
            Err(EgressDenied::ModelHostNotAllowed(host.to_string()))
        }
    }

    pub fn check(&self, provider: &Provider, kind: DataKind) -> Result<(), EgressDenied> {
        if !provider.is_remote() {
            return Ok(());
        }
        if self.local_only {
            return Err(EgressDenied::LocalOnly(provider.id().to_string()));
        }
        if !self.consents.contains(&(provider.id().to_string(), kind)) {
            return Err(EgressDenied::NoConsent(provider.id().to_string(), kind));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud() -> Provider {
        Provider::remote("remote-provider-a")
    }

    fn local() -> Provider {
        Provider::local("local-mt")
    }

    #[test]
    fn provider_constructors_set_the_remote_flag_and_the_getters_read_it() {
        let remote = Provider::remote("remote-provider-a");
        assert!(remote.is_remote());
        assert_eq!(remote.id(), "remote-provider-a");
        let local = Provider::local(String::from("local-mt"));
        assert!(!local.is_remote());
        assert_eq!(local.id(), "local-mt");
        assert_ne!(remote, Provider::local("remote-provider-a"));
    }

    #[test]
    fn a_provider_built_with_remote_is_checked_and_one_built_with_local_is_not() {
        let p = PrivacyPolicy::local_only();
        assert_eq!(
            p.check(&Provider::remote("x"), DataKind::Text),
            Err(EgressDenied::LocalOnly("x".into()))
        );
        assert_eq!(p.check(&Provider::local("x"), DataKind::Text), Ok(()));
    }

    #[test]
    fn local_provider_always_allowed() {
        let p = PrivacyPolicy::local_only();
        assert!(p.check(&local(), DataKind::Audio).is_ok());
    }

    #[test]
    fn local_only_blocks_remote_even_with_consent() {
        let mut p = PrivacyPolicy::local_only();
        p.grant("remote-provider-a", DataKind::Audio);
        assert_eq!(
            p.check(&cloud(), DataKind::Audio),
            Err(EgressDenied::LocalOnly("remote-provider-a".into()))
        );
    }

    #[test]
    fn remote_needs_consent_per_kind() {
        let mut p = PrivacyPolicy::default();
        p.set_local_only(false);
        p.grant("remote-provider-a", DataKind::Audio);
        assert!(p.check(&cloud(), DataKind::Audio).is_ok());
        assert_eq!(
            p.check(&cloud(), DataKind::Text),
            Err(EgressDenied::NoConsent(
                "remote-provider-a".into(),
                DataKind::Text
            ))
        );
    }

    #[test]
    fn model_host_passes_in_local_only_mode() {
        let mut p = PrivacyPolicy::local_only();
        p.allow_model_host("models.example.com")
            .expect("public host");
        assert!(p.check_model_download("models.example.com").is_ok());
    }

    #[test]
    fn model_host_must_be_on_the_list() {
        let p = PrivacyPolicy::local_only();
        assert_eq!(
            p.check_model_download("models.example.com"),
            Err(EgressDenied::ModelHostNotAllowed(
                "models.example.com".into()
            ))
        );
        let mut p = PrivacyPolicy::default();
        p.allow_model_host("models.example.com")
            .expect("public host");
        assert!(p.check_model_download("other.example.com").is_err());
        assert!(p.check_model_download("").is_err());
    }

    #[test]
    fn model_host_compare_ignores_case_and_revoke_works() {
        let mut p = PrivacyPolicy::local_only();
        p.allow_model_host("Models.Example.com")
            .expect("public host");
        assert!(p.check_model_download("models.EXAMPLE.com").is_ok());
        p.revoke_model_host("MODELS.example.com");
        assert!(p.check_model_download("models.example.com").is_err());
    }

    #[test]
    fn allowed_model_host_never_grants_user_data() {
        let host = Provider::remote("models.example.com");
        let mut open = PrivacyPolicy::local_only();
        open.set_local_only(false);
        for mut p in [PrivacyPolicy::local_only(), open] {
            p.allow_model_host("models.example.com")
                .expect("public host");
            for kind in [DataKind::Text, DataKind::Audio] {
                assert!(p.check(&host, kind).is_err(), "{kind:?} must stay denied");
            }
        }
        let mut p = PrivacyPolicy::local_only();
        p.allow_model_host("models.example.com")
            .expect("public host");
        assert_eq!(
            p.check(&host, DataKind::Text),
            Err(EgressDenied::LocalOnly("models.example.com".into()))
        );
    }

    #[test]
    fn revoke_removes_consent() {
        let mut p = PrivacyPolicy::default();
        p.set_local_only(false);
        p.grant("remote-provider-a", DataKind::Audio);
        p.revoke("remote-provider-a", DataKind::Audio);
        assert!(p.check(&cloud(), DataKind::Audio).is_err());
    }

    #[test]
    fn default_policy_equals_local_only_and_blocks_remote() {
        assert_eq!(PrivacyPolicy::default(), PrivacyPolicy::local_only());
        assert!(PrivacyPolicy::default().is_local_only());
        let mut p = PrivacyPolicy::default();
        p.grant("remote-provider-a", DataKind::Text);
        assert_eq!(
            p.check(&cloud(), DataKind::Text),
            Err(EgressDenied::LocalOnly("remote-provider-a".into()))
        );
    }

    #[test]
    fn local_only_flag_changes_only_through_the_setter() {
        let mut p = PrivacyPolicy::default();
        p.set_local_only(false);
        assert!(!p.is_local_only());
        p.grant("remote-provider-a", DataKind::Text);
        assert!(p.check(&cloud(), DataKind::Text).is_ok());
        p.set_local_only(true);
        assert!(p.check(&cloud(), DataKind::Text).is_err());
    }

    #[test]
    fn allow_model_host_rejects_unsafe_hosts() {
        let bad = [
            "",
            "   ",
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.20",
            "169.254.169.254",
            "0.0.0.0",
            "2130706433",
            "0x7f.0.0.1",
            "::1",
            "[::1]",
            "fe80::1",
            "localhost",
            "LocalHost",
            "localhost.",
            "api.localhost",
            "printer.local",
            "nas.lan",
            "db.internal",
            "router.home.arpa",
            "intranet",
            "models.example.com:8080",
            "models.example.com/path",
            "user@models.example.com",
            "https://models.example.com",
            "bad host.example.com",
            "-bad.example.com",
            "empty..label.example.com",
        ];
        for host in bad {
            let mut p = PrivacyPolicy::local_only();
            assert!(
                p.allow_model_host(host).is_err(),
                "{host:?} must be refused"
            );
            assert!(
                p.check_model_download(host).is_err(),
                "{host:?} must not pass the gate"
            );
        }
    }

    #[test]
    fn allow_model_host_accepts_public_names_and_keeps_case_folding() {
        let mut p = PrivacyPolicy::local_only();
        for host in [
            "models.example.com",
            "Cdn-1.Models.Example.COM",
            "xn--bcher-kva.example",
        ] {
            assert!(
                p.allow_model_host(host).is_ok(),
                "{host:?} must be accepted"
            );
            assert!(p.check_model_download(host).is_ok());
        }
        assert!(p.check_model_download("cdn-1.models.example.com").is_ok());
    }
}
