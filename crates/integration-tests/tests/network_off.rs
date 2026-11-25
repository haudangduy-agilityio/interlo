//! A guarded client over the `NetworkOff` transport: a request that the policy
//! refuses never reaches the transport, so `attempts()` stays 0.

use meeting_core::egress::test_support::{NetworkOff, RecordingLog};
use meeting_core::egress::{GuardedClient, Purpose, Request};
use meeting_core::policy::{PrivacyPolicy, Provider};
use meeting_core::protocol::DataKind;

#[test]
fn network_off_sees_no_attempt_after_a_refusal() {
    let off = NetworkOff::new();
    let log = RecordingLog::new();
    let client = GuardedClient::new(off.clone(), log.clone());
    let request = Request::new(
        Purpose::ProviderData {
            provider: Provider::remote("remote-provider-a"),
            kind: DataKind::Audio,
        },
        "https://provider.example.com/v1/audio",
    );
    let result = client.get(&PrivacyPolicy::default(), &request);
    assert!(result.is_err());
    assert_eq!(off.attempts(), 0);
    assert_eq!(log.events().len(), 1);
}
