//! Canonical meeting event protocol (draft v0.1), from the handoff spec
//! `specs/MEETING_CORE_PROTOCOL.md`, trimmed to the interpreter's needs.
//!
//! Wire format: an envelope with `eventType` and `payload`, the same shape
//! as the handoff examples. Every UI and worker boundary uses these types.

use serde::de::value::MapAccessDeserializer;
use serde::de::{DeserializeSeed, IntoDeserializer, MapAccess};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    En,
    Vi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentStatus {
    Partial,
    Stable,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranslationStatus {
    Pending,
    Partial,
    Final,
    Failed,
}

/// What leaves the machine. Used by the egress guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataKind {
    Audio,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Idle,
    Preparing,
    Running,
    Paused,
    Stopping,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub segment_id: String,
    /// "outgoing" (owner's mic, VI) or "incoming" (meeting audio, EN).
    pub source_id: String,
    pub revision: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub status: SegmentStatus,
    pub language: Language,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSegment {
    pub translation_id: String,
    pub source_segment_id: String,
    /// Exact source revision this translation was produced from.
    pub source_revision: u32,
    pub target_language: Language,
    pub text: String,
    pub status: TranslationStatus,
    pub provider_id: String,
    pub revision: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationFailure {
    pub translation_id: String,
    pub source_segment_id: String,
    pub source_revision: u32,
    pub provider_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "eventType", content = "payload")]
pub enum EventPayload {
    #[serde(rename = "transcript.segment.updated")]
    TranscriptSegmentUpdated(TranscriptSegment),

    #[serde(rename = "translation.segment.updated")]
    TranslationSegmentUpdated(TranslationSegment),

    #[serde(rename = "translation.segment.failed")]
    TranslationSegmentFailed(TranslationFailure),

    #[serde(rename = "session.status.changed")]
    SessionStatusChanged {
        from: SessionStatus,
        to: SessionStatus,
    },

    #[serde(rename = "speaker.label.updated")]
    SpeakerLabelUpdated {
        #[serde(rename = "speakerId")]
        speaker_id: String,
        label: String,
    },

    #[serde(rename = "privacy.egress.blocked")]
    EgressBlocked {
        #[serde(rename = "providerId")]
        provider_id: String,
        #[serde(rename = "dataKind")]
        data_kind: DataKind,
        reason: String,
    },
}

/// Envelope as it appears on the wire. `event_type` and `payload` are kept
/// as raw JSON here; `event()` decodes them into `EventPayload`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub schema_version: String,
    pub event_id: String,
    pub session_id: String,
    pub event_type: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub payload: Value,
}

impl Envelope {
    /// Build an envelope from a typed payload.
    pub fn new(
        event_id: impl Into<String>,
        session_id: impl Into<String>,
        sequence: u64,
        occurred_at: impl Into<String>,
        payload: &EventPayload,
    ) -> Result<Self, serde_json::Error> {
        let tagged = serde_json::to_value(payload)?;
        let event_type = tagged
            .get("eventType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let body = tagged.get("payload").cloned().unwrap_or(Value::Null);
        Ok(Self {
            schema_version: SCHEMA_VERSION.to_string(),
            event_id: event_id.into(),
            session_id: session_id.into(),
            event_type,
            sequence,
            occurred_at: occurred_at.into(),
            payload: body,
        })
    }

    /// Decode the typed payload. Fails on unknown event types. Reads the
    /// payload by reference: it is never cloned.
    pub fn event(&self) -> Result<EventPayload, serde_json::Error> {
        EventPayload::deserialize(MapAccessDeserializer::new(TaggedRef {
            event_type: &self.event_type,
            payload: &self.payload,
            next: 0,
        }))
    }
}

/// Presents `eventType` and `payload` as the two-entry map that the
/// adjacently tagged `EventPayload` expects, borrowing both values.
struct TaggedRef<'a> {
    event_type: &'a str,
    payload: &'a Value,
    next: u8,
}

impl<'de> MapAccess<'de> for TaggedRef<'de> {
    type Error = serde_json::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        let key = match self.next {
            0 => "eventType",
            1 => "payload",
            _ => return Ok(None),
        };
        seed.deserialize(IntoDeserializer::<serde_json::Error>::into_deserializer(
            key,
        ))
        .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        let which = self.next;
        self.next += 1;
        if which == 0 {
            seed.deserialize(IntoDeserializer::<serde_json::Error>::into_deserializer(
                self.event_type,
            ))
        } else {
            seed.deserialize(self.payload)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_variant() -> Vec<EventPayload> {
        vec![
            EventPayload::TranscriptSegmentUpdated(TranscriptSegment {
                segment_id: "s".into(),
                source_id: "incoming".into(),
                revision: 1,
                start_ms: 1,
                end_ms: 2,
                status: SegmentStatus::Stable,
                language: Language::En,
                text: "t".into(),
            }),
            EventPayload::TranslationSegmentUpdated(TranslationSegment {
                translation_id: "tr".into(),
                source_segment_id: "s".into(),
                source_revision: 1,
                target_language: Language::Vi,
                text: "d".into(),
                status: TranslationStatus::Partial,
                provider_id: "remote-provider-a".into(),
                revision: 1,
            }),
            EventPayload::TranslationSegmentFailed(TranslationFailure {
                translation_id: "tr".into(),
                source_segment_id: "s".into(),
                source_revision: 1,
                provider_id: "remote-provider-a".into(),
                reason: "timeout".into(),
            }),
            EventPayload::SessionStatusChanged {
                from: SessionStatus::Idle,
                to: SessionStatus::Preparing,
            },
            EventPayload::SpeakerLabelUpdated {
                speaker_id: "spk".into(),
                label: "Partner".into(),
            },
            EventPayload::EgressBlocked {
                provider_id: "remote-provider-a".into(),
                data_kind: DataKind::Text,
                reason: "local-only".into(),
            },
        ]
    }

    #[test]
    fn event_decodes_every_variant_it_encodes() {
        for payload in every_variant() {
            let env =
                Envelope::new("e", "s", 1, "2026-11-13T00:00:00.000Z", &payload).expect("encode");
            assert_eq!(env.event().expect("decode"), payload);
        }
    }

    #[test]
    fn event_rejects_unknown_types_and_malformed_payloads() {
        let mut env = Envelope::new("e", "s", 1, "t", &every_variant()[0]).expect("encode");
        let before = env.clone();
        env.payload = serde_json::json!({"segmentId": 7});
        assert!(env.event().is_err());
        env.payload = before.payload.clone();
        env.event_type = "transcript.segment.exploded".into();
        assert!(env.event().is_err());
        // Decoding never consumes or alters the envelope.
        env.event_type = before.event_type.clone();
        assert_eq!(env, before);
        assert!(env.event().is_ok());
    }
}
