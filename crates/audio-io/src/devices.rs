//! Device descriptions as the rest of the app sees them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Microphone,
    Headphones,
    BuiltInSpeakers,
    /// Virtual audio device with two channels (capture route for the meeting).
    VirtualInput2ch,
    /// Virtual audio device with sixteen output channels.
    VirtualOutput16ch,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
}

impl DeviceInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: DeviceKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fills_fields() {
        let d = DeviceInfo::new("dev-1", "Virtual 2ch", DeviceKind::VirtualInput2ch);
        assert_eq!(d.id, "dev-1");
        assert_eq!(d.name, "Virtual 2ch");
        assert_eq!(d.kind, DeviceKind::VirtualInput2ch);
    }

    #[test]
    fn kind_serializes_camel_case() {
        let json = serde_json::to_string(&DeviceKind::BuiltInSpeakers).unwrap();
        assert_eq!(json, "\"builtInSpeakers\"");
    }
}
