//! The policy-owned network client (ADR 0007, part 1).
//!
//! `GuardedClient` is the one way to reach a `Transport`. For every request it
//! checks, in this order (A-1 of the M1-12 SPEC): the url is `https://` with a
//! host; no meeting session is active (paused counts, D-103); then the rule of
//! the purpose in the privacy policy. A refusal logs one `privacy.egress.blocked`
//! event that holds an error code and nothing else, and never reaches the
//! transport.
//!
//! The transport takes a `Permit`, and only this module can build one, so code
//! that holds a transport still cannot call it around the guard.
//!
//! ```compile_fail,E0603
//! // No code outside this module can build a permit.
//! let _ = meeting_core::egress::Permit(());
//! ```
//!
//! ```compile_fail,E0277
//! // A permit has no `Default`, so it can not be made on demand either.
//! fn make<T: Default>() -> T {
//!     T::default()
//! }
//! let _ = make::<meeting_core::egress::Permit>();
//! ```
//!
//! ```compile_fail,E0061
//! // A transport can not be called without a permit.
//! use meeting_core::egress::{Permit, Transport, TransportError, TransportResponse};
//! struct Silent;
//! impl Transport for Silent {
//!     fn get(&self, _: &Permit, _: &str) -> Result<TransportResponse, TransportError> {
//!         Err(TransportError::Unreachable)
//!     }
//! }
//! let _ = Silent.get("https://models.example.com/a.bin");
//! ```
//!
//! The policy is passed by reference at every call. The app keeps one
//! `PrivacyPolicy` and passes that one: a clone made before a meeting starts
//! does not see the session status that is set later.

use crate::policy::{EgressDenied, PrivacyPolicy, Provider};
use crate::protocol::DataKind;
use diagnostics::{code, event, LogEvent};
use thiserror::Error;

/// What a request is for. The purpose picks the rule that the policy applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// User data (audio or text) for a remote provider: local-only mode and a
    /// consent per data kind decide.
    ProviderData { provider: Provider, kind: DataKind },
    /// A model download the owner started: the host must be on the model
    /// allow list.
    ModelDownload,
    /// A driver installer download the owner started: the host must be on the
    /// driver allow list.
    DriverDownload,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    pub purpose: Purpose,
    pub url: String,
}

/// Prints the kind of the purpose only. The url can hold a credential or a
/// token and the provider id names a service, so neither reaches a log line
/// that formats a request with `{:?}` (guardrail 6).
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let purpose = match self.purpose {
            Purpose::ProviderData { .. } => "ProviderData",
            Purpose::ModelDownload => "ModelDownload",
            Purpose::DriverDownload => "DriverDownload",
        };
        f.debug_struct("Request")
            .field("purpose", &purpose)
            .field("url", &"<hidden>")
            .finish()
    }
}

impl Request {
    pub fn new(purpose: Purpose, url: impl Into<String>) -> Self {
        Self {
            purpose,
            url: url.into(),
        }
    }
}

/// Proof that the guard let a request through. Only this module builds one:
/// the field is private and the type has no `Default` and no `Clone`.
#[derive(Debug)]
pub struct Permit(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("network unreachable")]
    Unreachable,
    #[error("transport failed")]
    Failed,
}

/// The only thing that touches a network. A real client implements it in the
/// task that owns the download (M1-04.4, M4-19); tests use `FakeTransport` or
/// `NetworkOff`. Starting shape: one GET (A-5 of the M1-12 SPEC).
///
/// One call is one request to one url. An implementation must not follow a
/// redirect: it returns the answer, and the caller sends the next hop through
/// the guard again (ADR 0021), so every hop is checked.
pub trait Transport {
    fn get(&self, permit: &Permit, url: &str) -> Result<TransportResponse, TransportError>;
}

/// One request of the model downloader (M1-04). The downloader fills it from the
/// compiled-in table only: the webview never supplies a url. It carries no
/// cookie, no credential and no body, so there is no field for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: String,
    /// Seconds from the start of the request until the secure connection is open.
    pub connect_timeout_secs: u64,
    /// Seconds allowed between two received chunks.
    pub read_timeout_secs: u64,
}

/// The answer to one request. The caller reads `body` in chunks it chooses and
/// follows a redirect itself, after it checked `location` with the policy.
pub struct FetchResponse {
    pub status: u16,
    /// The `Location` header of a redirect answer.
    pub location: Option<String>,
    pub body: Box<dyn std::io::Read + Send>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FetchError {
    #[error("the connection failed")]
    Connection,
    #[error("the connection timed out")]
    Timeout,
}

/// The fetch the model downloader uses. The real client (M1-04.4) is built from
/// a `&PrivacyPolicy` and lives in this crate; tests use a scripted fake. One
/// call is one request to one url, and an implementation must not follow a
/// redirect. This is the starting shape; M1-04.4 may extend it.
pub trait HttpFetch {
    fn get(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError>;
}

/// Receives the log event of each refusal.
pub trait EgressLog {
    fn record(&self, event: LogEvent);
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum GuardError {
    #[error("the policy refused the request: {0}")]
    Denied(EgressDenied),
    #[error("the url is not https with a host")]
    BadUrl,
    #[error("a local provider has no network path")]
    LocalProvider,
    #[error("the transport failed: {0}")]
    Transport(TransportError),
}

pub struct GuardedClient<T, L> {
    transport: T,
    log: L,
}

impl<T: Transport, L: EgressLog> GuardedClient<T, L> {
    pub fn new(transport: T, log: L) -> Self {
        Self { transport, log }
    }

    /// Send one GET if the policy allows it. `policy` is read at every call,
    /// so a change of the policy takes effect at the next request.
    pub fn get(
        &self,
        policy: &PrivacyPolicy,
        request: &Request,
    ) -> Result<TransportResponse, GuardError> {
        if let Err(refusal) = admit(policy, request) {
            // The one log event of a refusal: the event name and an error
            // code, nothing else (guardrail 6). No host, url, provider id or
            // text goes in.
            self.log
                .record(LogEvent::new(event::EGRESS_BLOCKED).with_code(refusal.code()));
            return Err(refusal.into());
        }
        self.transport
            .get(&Permit(()), &request.url)
            .map_err(GuardError::Transport)
    }
}

/// The text between `https://` and the first `/`, `?` or `#`. `None` when the
/// scheme is not exactly `https://` or the authority holds no host (nothing, or
/// only a user and a port such as `user@:443`).
fn https_authority(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let after_user = authority.rsplit('@').next().unwrap_or_default();
    let host = match after_user.strip_prefix('[') {
        Some(ipv6) => ipv6.split(']').next().unwrap_or_default(),
        None => after_user.split(':').next().unwrap_or_default(),
    };
    (!host.is_empty()).then_some(authority)
}

/// Why `admit` refused a request, before any transport was reached.
enum Refusal {
    BadUrl,
    LocalProvider,
    Denied(EgressDenied),
}

impl Refusal {
    /// The error code of the blocked event (A-4).
    fn code(&self) -> &'static str {
        match self {
            Self::BadUrl => code::EGRESS_BAD_URL,
            Self::LocalProvider => code::EGRESS_LOCAL_PROVIDER,
            Self::Denied(EgressDenied::LocalOnly(_)) => code::EGRESS_LOCAL_ONLY,
            Self::Denied(EgressDenied::NoConsent(..)) => code::EGRESS_NO_CONSENT,
            Self::Denied(EgressDenied::MeetingRunning) => code::EGRESS_MEETING_RUNNING,
            Self::Denied(
                EgressDenied::ModelHostNotAllowed(_) | EgressDenied::DriverHostNotAllowed(_),
            ) => code::EGRESS_HOST_NOT_ALLOWED,
        }
    }
}

impl From<Refusal> for GuardError {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::BadUrl => Self::BadUrl,
            Refusal::LocalProvider => Self::LocalProvider,
            Refusal::Denied(denied) => Self::Denied(denied),
        }
    }
}

/// The checks of A-1, in order. The first that fails names the refusal.
fn admit(policy: &PrivacyPolicy, request: &Request) -> Result<(), Refusal> {
    let authority = https_authority(&request.url).ok_or(Refusal::BadUrl)?;
    if policy.is_session_active() {
        return Err(Refusal::Denied(EgressDenied::MeetingRunning));
    }
    match &request.purpose {
        Purpose::ProviderData { provider, kind } => {
            // A local provider never needs the network, and the policy allows
            // it everywhere. The guard has no path for it, so a caller can not
            // skip the guard by declaring its provider local.
            if !provider.is_remote() {
                return Err(Refusal::LocalProvider);
            }
            policy.check(provider, *kind)
        }
        Purpose::ModelDownload => policy.check_model_download(authority),
        Purpose::DriverDownload => policy.check_driver_download(authority),
    }
    .map_err(Refusal::Denied)
}

/// Test helpers. Built for this crate's tests and, with the cargo feature
/// `test-support`, for the tests of other crates. Never in the product build.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    /// A transport that records the urls it gets and answers with a fixed
    /// reply. Clones share the record.
    #[derive(Clone)]
    pub struct FakeTransport {
        calls: Arc<Mutex<Vec<String>>>,
        reply: Result<TransportResponse, TransportError>,
    }

    impl FakeTransport {
        pub fn ok() -> Self {
            Self::replying(Ok(TransportResponse {
                status: 200,
                body: Vec::new(),
            }))
        }

        pub fn replying(reply: Result<TransportResponse, TransportError>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                reply,
            }
        }

        /// The urls that reached the transport, in order.
        pub fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl Transport for FakeTransport {
        fn get(&self, _permit: &Permit, url: &str) -> Result<TransportResponse, TransportError> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(url.to_string());
            self.reply.clone()
        }
    }

    /// The network-off transport: every attempt fails and is counted. A test
    /// that expects no network call asserts `attempts() == 0`. Clones share
    /// the counter.
    #[derive(Clone, Default)]
    pub struct NetworkOff {
        attempts: Arc<AtomicUsize>,
    }

    impl NetworkOff {
        pub fn new() -> Self {
            Self::default()
        }

        /// How many times a request reached this transport.
        pub fn attempts(&self) -> usize {
            self.attempts.load(Ordering::SeqCst)
        }
    }

    impl Transport for NetworkOff {
        fn get(&self, _permit: &Permit, _url: &str) -> Result<TransportResponse, TransportError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(TransportError::Unreachable)
        }
    }

    /// Keeps the events it gets. Clones share the record.
    #[derive(Clone, Default)]
    pub struct RecordingLog {
        events: Arc<Mutex<Vec<LogEvent>>>,
    }

    impl RecordingLog {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn events(&self) -> Vec<LogEvent> {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl EgressLog for RecordingLog {
        fn record(&self, event: LogEvent) {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{FakeTransport, NetworkOff, RecordingLog};
    use super::*;
    use crate::protocol::SessionStatus;

    const MODEL_URL: &str = "https://models.example.com/a.bin";
    const DRIVER_URL: &str = "https://drivers.example.com/d.pkg";

    fn remote() -> Provider {
        Provider::remote("p")
    }

    fn provider_text() -> Purpose {
        Purpose::ProviderData {
            provider: remote(),
            kind: DataKind::Text,
        }
    }

    fn rig() -> (
        GuardedClient<FakeTransport, RecordingLog>,
        FakeTransport,
        RecordingLog,
    ) {
        let transport = FakeTransport::ok();
        let log = RecordingLog::new();
        (
            GuardedClient::new(transport.clone(), log.clone()),
            transport,
            log,
        )
    }

    /// local-only off, consent for `p` and `Text`, one host on each list.
    fn open_policy() -> PrivacyPolicy {
        let mut policy = PrivacyPolicy::default();
        policy.set_local_only(false);
        policy.grant("p", DataKind::Text);
        policy
            .allow_model_host("models.example.com")
            .expect("public host");
        policy
            .allow_driver_host("drivers.example.com")
            .expect("public host");
        policy
    }

    fn codes(log: &RecordingLog) -> Vec<Option<&'static str>> {
        log.events().iter().map(LogEvent::code).collect()
    }

    fn assert_one_block(log: &RecordingLog, expected: &'static str) {
        let events = log.events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].name(), event::EGRESS_BLOCKED);
        assert_eq!(events[0].code(), Some(expected));
    }

    #[test]
    fn default_policy_refuses_a_remote_provider_and_logs_local_only() {
        let (client, transport, log) = rig();
        let result = client.get(
            &PrivacyPolicy::default(),
            &Request::new(provider_text(), MODEL_URL),
        );
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::LocalOnly("p".into())))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_LOCAL_ONLY);
    }

    #[test]
    fn local_only_refuses_a_remote_provider_with_consent() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::local_only();
        policy.grant("p", DataKind::Audio);
        let request = Request::new(
            Purpose::ProviderData {
                provider: remote(),
                kind: DataKind::Audio,
            },
            MODEL_URL,
        );
        assert!(client.get(&policy, &request).is_err());
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_LOCAL_ONLY);
    }

    #[test]
    fn remote_provider_without_consent_is_refused_with_no_consent_code() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy.set_local_only(false);
        let result = client.get(&policy, &Request::new(provider_text(), MODEL_URL));
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::NoConsent(
                "p".into(),
                DataKind::Text
            )))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_NO_CONSENT);
    }

    #[test]
    fn remote_provider_with_consent_reaches_the_transport_once() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy.set_local_only(false);
        policy.grant("p", DataKind::Text);
        let result = client.get(&policy, &Request::new(provider_text(), MODEL_URL));
        assert_eq!(result.map(|r| r.status), Ok(200));
        assert_eq!(transport.calls(), vec![MODEL_URL.to_string()]);
        assert!(log.events().is_empty());
    }

    #[test]
    fn a_local_provider_is_refused_before_the_policy_rule() {
        let local_text = || Purpose::ProviderData {
            provider: Provider::local("l"),
            kind: DataKind::Text,
        };
        // The default policy allows a local provider, so only the guard stops it.
        let (client, transport, log) = rig();
        let result = client.get(
            &PrivacyPolicy::default(),
            &Request::new(local_text(), MODEL_URL),
        );
        assert_eq!(result, Err(GuardError::LocalProvider));
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_LOCAL_PROVIDER);

        // The session check comes first.
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy.set_session_status(SessionStatus::Running);
        let result = client.get(&policy, &Request::new(local_text(), MODEL_URL));
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::MeetingRunning))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_MEETING_RUNNING);

        // The url check comes first.
        let (client, transport, log) = rig();
        let result = client.get(
            &PrivacyPolicy::default(),
            &Request::new(local_text(), "http://models.example.com/a.bin"),
        );
        assert_eq!(result, Err(GuardError::BadUrl));
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_BAD_URL);
    }

    #[test]
    fn model_host_off_the_allow_list_is_refused_and_logged() {
        let (client, transport, log) = rig();
        let result = client.get(
            &PrivacyPolicy::default(),
            &Request::new(Purpose::ModelDownload, MODEL_URL),
        );
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::ModelHostNotAllowed(
                "models.example.com".into()
            )))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_HOST_NOT_ALLOWED);
    }

    #[test]
    fn listed_model_host_passes_in_local_only_mode_ignoring_case_path_and_query() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_model_host("models.example.com")
            .expect("public host");
        assert!(policy.is_local_only());
        let urls = [
            "https://Models.Example.com/path/a.bin?x=1",
            "https://models.example.com",
            "https://models.example.com?x=1",
            "https://models.example.com#f",
        ];
        for url in urls {
            let result = client.get(&policy, &Request::new(Purpose::ModelDownload, url));
            assert!(result.is_ok(), "{url}");
        }
        assert_eq!(transport.calls().len(), 4);
        assert!(log.events().is_empty());
    }

    #[test]
    fn listed_driver_host_passes_in_local_only_mode() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_driver_host("drivers.example.com")
            .expect("public host");
        let result = client.get(&policy, &Request::new(Purpose::DriverDownload, DRIVER_URL));
        assert!(result.is_ok());
        assert_eq!(transport.calls(), vec![DRIVER_URL.to_string()]);
        assert!(log.events().is_empty());
    }

    #[test]
    fn a_model_host_is_refused_as_a_driver_host() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_model_host("models.example.com")
            .expect("public host");
        let result = client.get(&policy, &Request::new(Purpose::DriverDownload, MODEL_URL));
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::DriverHostNotAllowed(
                "models.example.com".into()
            )))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_HOST_NOT_ALLOWED);
    }

    #[test]
    fn a_driver_host_is_refused_as_a_model_host() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_driver_host("drivers.example.com")
            .expect("public host");
        let result = client.get(&policy, &Request::new(Purpose::ModelDownload, DRIVER_URL));
        assert_eq!(
            result,
            Err(GuardError::Denied(EgressDenied::ModelHostNotAllowed(
                "drivers.example.com".into()
            )))
        );
        assert!(transport.calls().is_empty());
        assert_one_block(&log, code::EGRESS_HOST_NOT_ALLOWED);
    }

    #[test]
    fn a_url_that_is_not_a_usable_https_url_is_refused_for_every_purpose() {
        let (client, transport, log) = rig();
        let policy = open_policy();
        let bad = [
            "http://models.example.com/a.bin",
            "ftp://models.example.com/a.bin",
            "models.example.com/a.bin",
            "https:///a.bin",
        ];
        let purposes = [
            provider_text(),
            Purpose::ModelDownload,
            Purpose::DriverDownload,
        ];
        for url in bad {
            for purpose in &purposes {
                let result = client.get(&policy, &Request::new(purpose.clone(), url));
                assert_eq!(result, Err(GuardError::BadUrl), "{url} {purpose:?}");
            }
        }
        let upper = client.get(
            &policy,
            &Request::new(Purpose::ModelDownload, "HTTPS://models.example.com/a.bin"),
        );
        assert_eq!(upper, Err(GuardError::BadUrl));
        assert!(transport.calls().is_empty());
        // 4 bad urls x 3 purposes, plus the upper-case scheme.
        assert_eq!(codes(&log), vec![Some(code::EGRESS_BAD_URL); 4 * 3 + 1]);

        // The url check comes before the policy check (A-1).
        let (client, _, log) = rig();
        let result = client.get(
            &PrivacyPolicy::default(),
            &Request::new(provider_text(), "http://x.example.com/"),
        );
        assert_eq!(result, Err(GuardError::BadUrl));
        assert_one_block(&log, code::EGRESS_BAD_URL);
    }

    #[test]
    fn an_authority_without_a_host_is_a_bad_url() {
        let (client, transport, log) = rig();
        let policy = open_policy();
        let urls = [
            "https://:443/x",
            "https://user@/x",
            "https://@/x",
            "https://user:pw@:8080/x",
        ];
        let purposes = [
            provider_text(),
            Purpose::ModelDownload,
            Purpose::DriverDownload,
        ];
        for url in urls {
            for purpose in &purposes {
                let result = client.get(&policy, &Request::new(purpose.clone(), url));
                assert_eq!(result, Err(GuardError::BadUrl), "{url} {purpose:?}");
            }
        }
        assert!(transport.calls().is_empty());
        assert_eq!(
            codes(&log),
            vec![Some(code::EGRESS_BAD_URL); urls.len() * purposes.len()]
        );
    }

    #[test]
    fn a_request_prints_no_url_and_no_provider_id_in_debug() {
        let request = Request::new(
            Purpose::ProviderData {
                provider: Provider::remote("secret-provider"),
                kind: DataKind::Text,
            },
            "https://user:secret-pw@secret-host.example/private?token=abc",
        );
        let printed = format!("{request:?}");
        for secret in ["secret-pw", "secret-host", "secret-provider", "token"] {
            assert!(!printed.contains(secret), "{secret} in {printed}");
        }
    }

    #[test]
    fn the_whole_authority_must_match_the_listed_host() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_model_host("models.example.com")
            .expect("public host");
        let urls = [
            "https://models.example.com@evil.example/a",
            "https://models.example.com:8443/a",
            "https://evil.example\\@models.example.com/a",
            "https://models.example.com.evil.example/a",
            "https://models.example.com%2F@evil.example/a",
        ];
        for url in urls {
            let result = client.get(&policy, &Request::new(Purpose::ModelDownload, url));
            assert!(
                matches!(
                    result,
                    Err(GuardError::Denied(EgressDenied::ModelHostNotAllowed(_)))
                ),
                "{url}: {result:?}"
            );
        }
        assert!(transport.calls().is_empty());
        assert_eq!(
            codes(&log),
            vec![Some(code::EGRESS_HOST_NOT_ALLOWED); urls.len()]
        );
    }

    #[test]
    fn the_first_failing_check_names_the_code() {
        let (client, transport, log) = rig();
        let mut policy = PrivacyPolicy::default();
        policy.set_session_status(SessionStatus::Running);
        let bad_url = Request::new(Purpose::ModelDownload, "http://models.example.com/a");
        assert_eq!(client.get(&policy, &bad_url), Err(GuardError::BadUrl));
        let busy = Request::new(Purpose::ModelDownload, "https://models.example.com/a");
        assert_eq!(
            client.get(&policy, &busy),
            Err(GuardError::Denied(EgressDenied::MeetingRunning))
        );
        assert!(transport.calls().is_empty());
        assert_eq!(
            codes(&log),
            vec![
                Some(code::EGRESS_BAD_URL),
                Some(code::EGRESS_MEETING_RUNNING)
            ]
        );
    }

    fn three_requests() -> [Request; 3] {
        [
            Request::new(provider_text(), MODEL_URL),
            Request::new(Purpose::ModelDownload, MODEL_URL),
            Request::new(Purpose::DriverDownload, DRIVER_URL),
        ]
    }

    #[test]
    fn every_purpose_is_refused_while_the_session_is_active() {
        let (client, transport, log) = rig();
        let mut policy = open_policy();
        for status in [
            SessionStatus::Preparing,
            SessionStatus::Running,
            SessionStatus::Paused,
            SessionStatus::Stopping,
        ] {
            policy.set_session_status(status);
            for request in three_requests() {
                let result = client.get(&policy, &request);
                assert_eq!(
                    result,
                    Err(GuardError::Denied(EgressDenied::MeetingRunning)),
                    "{status:?} {request:?}"
                );
            }
        }
        assert!(transport.calls().is_empty());
        // 4 active statuses x 3 requests.
        assert_eq!(codes(&log), vec![Some(code::EGRESS_MEETING_RUNNING); 4 * 3]);
    }

    #[test]
    fn the_network_opens_again_when_the_session_is_idle_completed_or_failed() {
        let (client, transport, log) = rig();
        let mut policy = open_policy();
        for status in [
            SessionStatus::Idle,
            SessionStatus::Completed,
            SessionStatus::Failed,
        ] {
            policy.set_session_status(status);
            for request in three_requests() {
                assert!(client.get(&policy, &request).is_ok(), "{status:?}");
            }
        }
        assert_eq!(transport.calls().len(), 9);
        assert!(log.events().is_empty());
    }

    #[test]
    fn a_refusal_ends_when_the_session_ends() {
        let (client, transport, _log) = rig();
        let mut policy = open_policy();
        let request = Request::new(Purpose::ModelDownload, MODEL_URL);
        policy.set_session_status(SessionStatus::Running);
        assert!(client.get(&policy, &request).is_err());
        policy.set_session_status(SessionStatus::Idle);
        assert!(client.get(&policy, &request).is_ok());
        assert_eq!(transport.calls().len(), 1);
    }

    #[test]
    fn a_transport_failure_is_passed_on_and_not_logged_as_a_block() {
        let transport = FakeTransport::replying(Err(TransportError::Unreachable));
        let log = RecordingLog::new();
        let client = GuardedClient::new(transport.clone(), log.clone());
        let result = client.get(
            &open_policy(),
            &Request::new(Purpose::ModelDownload, MODEL_URL),
        );
        assert_eq!(
            result,
            Err(GuardError::Transport(TransportError::Unreachable))
        );
        assert_eq!(transport.calls().len(), 1);
        assert!(log.events().is_empty());
    }

    #[test]
    fn blocked_events_carry_no_host_url_provider_or_text() {
        let (client, _transport, log) = rig();
        let secret = "secret-host.example/private/path?token=abc";
        let https = format!("https://{secret}");
        let provider = Purpose::ProviderData {
            provider: Provider::remote("secret-provider"),
            kind: DataKind::Text,
        };
        let model = Purpose::ModelDownload;

        let _ = client.get(
            &PrivacyPolicy::default(),
            &Request::new(provider.clone(), https.clone()),
        );
        let mut no_consent = PrivacyPolicy::default();
        no_consent.set_local_only(false);
        let _ = client.get(&no_consent, &Request::new(provider.clone(), https.clone()));
        let _ = client.get(
            &PrivacyPolicy::default(),
            &Request::new(model.clone(), https.clone()),
        );
        for scheme in ["http://", "ftp://", "", "https:///"] {
            let _ = client.get(
                &PrivacyPolicy::default(),
                &Request::new(model.clone(), format!("{scheme}{secret}")),
            );
        }
        let mut busy = PrivacyPolicy::default();
        busy.set_local_only(false);
        busy.grant("secret-provider", DataKind::Text);
        busy.allow_model_host("secret-host.example")
            .expect("public host");
        busy.allow_driver_host("secret-host.example")
            .expect("public host");
        busy.set_session_status(SessionStatus::Running);
        for purpose in [provider, model, Purpose::DriverDownload] {
            let _ = client.get(&busy, &Request::new(purpose, https.clone()));
        }

        let events = log.events();
        // 1 + 1 + 1 + 4 bad urls + 3 purposes while the session is active.
        assert_eq!(events.len(), 10);
        let out = diagnostics::export_json_lines(&events, true).expect("export");
        for line in out.lines() {
            assert!(line.contains(event::EGRESS_BLOCKED), "{line}");
            assert!(line.contains("\"code\":\"E_EGRESS_"), "{line}");
        }
        for secret in ["secret-host", "secret-provider", "private", "token"] {
            assert!(!out.contains(secret), "{secret} leaked: {out}");
        }
    }

    #[test]
    fn a_refusal_does_not_change_the_policy() {
        let (client, _transport, _log) = rig();
        let setups: Vec<(PrivacyPolicy, Request)> = {
            let mut busy = open_policy();
            busy.set_session_status(SessionStatus::Running);
            let mut no_consent = PrivacyPolicy::default();
            no_consent.set_local_only(false);
            vec![
                (
                    PrivacyPolicy::default(),
                    Request::new(provider_text(), MODEL_URL),
                ),
                (no_consent, Request::new(provider_text(), MODEL_URL)),
                (
                    PrivacyPolicy::default(),
                    Request::new(Purpose::ModelDownload, MODEL_URL),
                ),
                (busy, Request::new(Purpose::ModelDownload, MODEL_URL)),
            ]
        };
        for (policy, request) in setups {
            let before = policy.clone();
            assert!(client.get(&policy, &request).is_err());
            assert_eq!(policy, before);
        }
    }

    #[test]
    fn network_off_counts_attempts() {
        let off = NetworkOff::new();
        let client = GuardedClient::new(off.clone(), RecordingLog::new());
        let mut policy = PrivacyPolicy::default();
        policy
            .allow_model_host("models.example.com")
            .expect("public host");
        let refused = client.get(&policy, &Request::new(provider_text(), MODEL_URL));
        assert!(refused.is_err());
        assert_eq!(off.attempts(), 0);
        let allowed = client.get(&policy, &Request::new(Purpose::ModelDownload, MODEL_URL));
        assert_eq!(
            allowed,
            Err(GuardError::Transport(TransportError::Unreachable))
        );
        assert_eq!(off.attempts(), 1);
    }
}
