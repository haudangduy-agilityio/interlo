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

/// How a device is connected, as the system reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    BuiltIn,
    Virtual,
    Bluetooth,
    Usb,
    /// HDMI, DisplayPort and other display outputs.
    Display,
    /// Any other value, or none reported.
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    #[serde(default)]
    pub input_channels: u32,
    #[serde(default)]
    pub output_channels: u32,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default)]
    pub is_default_input: bool,
    #[serde(default)]
    pub is_default_output: bool,
}

impl DeviceInfo {
    /// A device with no channels, no transport and no default role. Use the
    /// `with_*` methods to fill what the system reported.
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: DeviceKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            input_channels: 0,
            output_channels: 0,
            transport: Transport::Other,
            is_default_input: false,
            is_default_output: false,
        }
    }

    pub fn with_channels(mut self, input_channels: u32, output_channels: u32) -> Self {
        self.input_channels = input_channels;
        self.output_channels = output_channels;
        self
    }

    pub fn with_transport(mut self, transport: Transport) -> Self {
        self.transport = transport;
        self
    }

    pub fn with_defaults(mut self, is_default_input: bool, is_default_output: bool) -> Self {
        self.is_default_input = is_default_input;
        self.is_default_output = is_default_output;
        self
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
    fn new_keeps_the_old_values_and_adds_neutral_new_fields() {
        let d = DeviceInfo::new("dev-1", "Some device", DeviceKind::Other);
        assert_eq!((d.input_channels, d.output_channels), (0, 0));
        assert_eq!(d.transport, Transport::Other);
        assert!(!d.is_default_input && !d.is_default_output);
    }

    #[test]
    fn with_methods_set_only_their_fields() {
        let d = DeviceInfo::new("d", "D", DeviceKind::Headphones)
            .with_channels(1, 2)
            .with_transport(Transport::Bluetooth)
            .with_defaults(false, true);
        assert_eq!((d.input_channels, d.output_channels), (1, 2));
        assert_eq!(d.transport, Transport::Bluetooth);
        assert!(!d.is_default_input && d.is_default_output);
        assert_eq!(d.kind, DeviceKind::Headphones);
    }

    #[test]
    fn old_json_without_the_new_fields_still_reads() {
        let d: DeviceInfo =
            serde_json::from_str(r#"{"id":"a","name":"A","kind":"microphone"}"#).unwrap();
        assert_eq!(d.transport, Transport::Other);
        assert_eq!(d.input_channels, 0);
    }

    #[test]
    fn kind_serializes_camel_case() {
        let json = serde_json::to_string(&DeviceKind::BuiltInSpeakers).unwrap();
        assert_eq!(json, "\"builtInSpeakers\"");
    }
}
