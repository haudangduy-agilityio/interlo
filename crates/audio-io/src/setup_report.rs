//! The setup report: eight rows, each with a state, a detail line, a typed fix
//! when the row is not ok, and a warning where one applies (FR-01, FR-43).
//!
//! A pure function of facts. Where a fact comes from (the system tool, the
//! permission probe, the model manager) is not decided here.

use serde::Serialize;

use crate::device_list::DeviceSnapshot;
use crate::devices::{DeviceInfo, DeviceKind, Transport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SetupItemId {
    #[serde(rename = "mic")]
    Mic,
    #[serde(rename = "capture")]
    Capture,
    #[serde(rename = "vmic")]
    Vmic,
    #[serde(rename = "phones")]
    Phones,
    #[serde(rename = "sr")]
    Sr,
    #[serde(rename = "tr")]
    Tr,
    #[serde(rename = "enV")]
    EnV,
    #[serde(rename = "viV")]
    ViV,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupState {
    Ok,
    Missing,
    Installing,
    Error,
}

/// What the owner does about a row that is not ok. The English text lives in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupFix {
    AllowMicrophone,
    ConnectMicrophone,
    InstallCaptureDevice,
    InstallVirtualMicrophone,
    ConnectHeadphones,
    DownloadModel,
    WaitForInstall,
    CheckAgain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupWarning {
    /// Bluetooth output can add delay and lower microphone quality (FR-43).
    BluetoothOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermission {
    Granted,
    Denied,
    NotDetermined,
    /// The caller could not read the permission.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Ok,
    Missing,
    Downloading,
    Error,
}

/// Why opening the microphone failed, as a backend reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InputOpenError {
    #[error("microphone permission denied")]
    PermissionDenied,
    #[error("input device not found")]
    DeviceNotFound,
    #[error("input device could not be opened")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub id: SetupItemId,
    pub state: SetupState,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<SetupFix>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<SetupWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupReport {
    pub items: Vec<SetupItem>,
}

/// Everything the report needs. Models are in the order `sr`, `tr`, `enV`, `viV`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupFacts {
    pub devices: DeviceSnapshot,
    pub tap_available: bool,
    pub mic_permission: MicPermission,
    pub models: [ModelState; 4],
}

/// A denied permission is the one open error that tells the owner what to do.
/// Any other error says nothing about the permission.
pub fn mic_permission_from_error(error: InputOpenError) -> MicPermission {
    match error {
        InputOpenError::PermissionDenied => MicPermission::Denied,
        InputOpenError::DeviceNotFound | InputOpenError::Other => MicPermission::Unknown,
    }
}

fn item(
    id: SetupItemId,
    state: SetupState,
    detail: impl Into<String>,
    fix: Option<SetupFix>,
) -> SetupItem {
    SetupItem {
        id,
        state,
        detail: detail.into(),
        fix,
        warning: None,
    }
}

fn is_virtual(d: &DeviceInfo) -> bool {
    d.transport == Transport::Virtual
}

/// The default input when it is not virtual, else the first non-virtual input.
pub fn microphone(devices: &[DeviceInfo]) -> Option<&DeviceInfo> {
    let usable = |d: &&DeviceInfo| d.input_channels > 0 && !is_virtual(d);
    devices
        .iter()
        .filter(usable)
        .find(|d| d.is_default_input)
        .or_else(|| devices.iter().find(usable))
}

fn mic_row(facts: &SetupFacts) -> SetupItem {
    let Some(device) = microphone(&facts.devices.devices) else {
        return item(
            SetupItemId::Mic,
            SetupState::Missing,
            "No microphone found",
            Some(SetupFix::ConnectMicrophone),
        );
    };
    match facts.mic_permission {
        MicPermission::Granted => item(SetupItemId::Mic, SetupState::Ok, device.name.clone(), None),
        MicPermission::Denied => item(
            SetupItemId::Mic,
            SetupState::Missing,
            "Microphone access is off",
            Some(SetupFix::AllowMicrophone),
        ),
        MicPermission::NotDetermined => item(
            SetupItemId::Mic,
            SetupState::Missing,
            "Microphone access has not been asked yet",
            Some(SetupFix::AllowMicrophone),
        ),
        MicPermission::Unknown => item(
            SetupItemId::Mic,
            SetupState::Error,
            "Microphone permission could not be checked",
            Some(SetupFix::CheckAgain),
        ),
    }
}

/// Decided order (D-15): the process tap, then the 2-channel virtual device.
fn capture_row(facts: &SetupFacts) -> SetupItem {
    if facts.tap_available {
        return item(SetupItemId::Capture, SetupState::Ok, "Process tap", None);
    }
    let has_2ch = facts
        .devices
        .devices
        .iter()
        .any(|d| d.kind == DeviceKind::VirtualInput2ch);
    if has_2ch {
        item(
            SetupItemId::Capture,
            SetupState::Ok,
            "2-channel virtual device",
            None,
        )
    } else {
        item(
            SetupItemId::Capture,
            SetupState::Missing,
            "No capture route works",
            Some(SetupFix::InstallCaptureDevice),
        )
    }
}

fn vmic_row(facts: &SetupFacts) -> SetupItem {
    let present = facts
        .devices
        .devices
        .iter()
        .any(|d| d.kind == DeviceKind::VirtualOutput16ch);
    if present {
        item(
            SetupItemId::Vmic,
            SetupState::Ok,
            "16-channel virtual device",
            None,
        )
    } else {
        item(
            SetupItemId::Vmic,
            SetupState::Missing,
            "16-channel virtual device not installed",
            Some(SetupFix::InstallVirtualMicrophone),
        )
    }
}

fn phones_row(facts: &SetupFacts) -> SetupItem {
    let output = facts.devices.devices.iter().find(|d| d.is_default_output);
    let mut r = match output {
        Some(d) if d.kind == DeviceKind::Headphones => item(
            SetupItemId::Phones,
            SetupState::Ok,
            "Headphones connected",
            None,
        ),
        _ => item(
            SetupItemId::Phones,
            SetupState::Missing,
            "No headphones detected",
            Some(SetupFix::ConnectHeadphones),
        ),
    };
    if output.is_some_and(|d| d.transport == Transport::Bluetooth) {
        r.warning = Some(SetupWarning::BluetoothOutput);
    }
    r
}

fn model_row(id: SetupItemId, state: ModelState, label: &str) -> SetupItem {
    match state {
        ModelState::Ok => item(id, SetupState::Ok, label, None),
        ModelState::Missing => item(
            id,
            SetupState::Missing,
            format!("{label} not downloaded"),
            Some(SetupFix::DownloadModel),
        ),
        ModelState::Downloading => item(
            id,
            SetupState::Installing,
            format!("{label} downloading"),
            Some(SetupFix::WaitForInstall),
        ),
        ModelState::Error => item(
            id,
            SetupState::Error,
            format!("{label} failed"),
            Some(SetupFix::DownloadModel),
        ),
    }
}

/// Builds the eight rows in the order `mic`, `capture`, `vmic`, `phones`, `sr`,
/// `tr`, `enV`, `viV`. Headphones never block readiness; the report has no
/// field that says so, because that rule belongs to the start gate.
pub fn build_report(facts: &SetupFacts) -> SetupReport {
    let [sr, tr, en_v, vi_v] = facts.models;
    SetupReport {
        items: vec![
            mic_row(facts),
            capture_row(facts),
            vmic_row(facts),
            phones_row(facts),
            model_row(SetupItemId::Sr, sr, "Speech recognition model"),
            model_row(SetupItemId::Tr, tr, "Translation model"),
            model_row(SetupItemId::EnV, en_v, "English voice model"),
            model_row(SetupItemId::ViV, vi_v, "Vietnamese voice model"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(
        name: &str,
        kind: DeviceKind,
        input: u32,
        output: u32,
        transport: Transport,
    ) -> DeviceInfo {
        DeviceInfo::new(name, name, kind)
            .with_channels(input, output)
            .with_transport(transport)
    }

    fn mic() -> DeviceInfo {
        dev("Mic", DeviceKind::Microphone, 1, 0, Transport::BuiltIn).with_defaults(true, false)
    }
    fn v2() -> DeviceInfo {
        dev("V2", DeviceKind::VirtualInput2ch, 2, 2, Transport::Virtual)
    }
    fn v16() -> DeviceInfo {
        dev(
            "V16",
            DeviceKind::VirtualOutput16ch,
            16,
            16,
            Transport::Virtual,
        )
    }
    fn phones() -> DeviceInfo {
        dev("Phones", DeviceKind::Headphones, 0, 2, Transport::BuiltIn).with_defaults(false, true)
    }

    fn facts_with(devices: Vec<DeviceInfo>) -> SetupFacts {
        SetupFacts {
            devices: DeviceSnapshot {
                devices,
                skipped: 0,
            },
            tap_available: true,
            mic_permission: MicPermission::Granted,
            models: [ModelState::Ok; 4],
        }
    }

    fn all_ok() -> SetupFacts {
        facts_with(vec![mic(), v2(), v16(), phones()])
    }

    fn find_item(report: &SetupReport, id: SetupItemId) -> &SetupItem {
        report
            .items
            .iter()
            .find(|r| r.id == id)
            .expect("row present")
    }

    #[test]
    fn all_ok_gives_eight_rows_in_order() {
        let report = build_report(&all_ok());
        let ids: Vec<SetupItemId> = report.items.iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                SetupItemId::Mic,
                SetupItemId::Capture,
                SetupItemId::Vmic,
                SetupItemId::Phones,
                SetupItemId::Sr,
                SetupItemId::Tr,
                SetupItemId::EnV,
                SetupItemId::ViV
            ]
        );
        for r in &report.items {
            assert_eq!(r.state, SetupState::Ok, "{:?}", r.id);
            assert_eq!(r.fix, None, "{:?}", r.id);
            assert_eq!(r.warning, None, "{:?}", r.id);
        }
    }

    #[test]
    fn mic_denied_is_missing_with_the_allow_fix() {
        let mut f = all_ok();
        f.mic_permission = MicPermission::Denied;
        let r = build_report(&f);
        let m = find_item(&r, SetupItemId::Mic);
        assert_eq!(
            (m.state, m.fix),
            (SetupState::Missing, Some(SetupFix::AllowMicrophone))
        );
    }

    #[test]
    fn mic_not_asked_yet_has_the_allow_fix_and_its_own_detail() {
        let mut denied = all_ok();
        denied.mic_permission = MicPermission::Denied;
        let mut asked = all_ok();
        asked.mic_permission = MicPermission::NotDetermined;
        let (d, a) = (build_report(&denied), build_report(&asked));
        let (d, a) = (
            find_item(&d, SetupItemId::Mic),
            find_item(&a, SetupItemId::Mic),
        );
        assert_eq!(
            (a.state, a.fix),
            (SetupState::Missing, Some(SetupFix::AllowMicrophone))
        );
        assert_ne!(a.detail, d.detail);
    }

    #[test]
    fn mic_without_an_input_device_is_missing_whatever_the_permission() {
        for permission in [
            MicPermission::Granted,
            MicPermission::Denied,
            MicPermission::NotDetermined,
            MicPermission::Unknown,
        ] {
            let mut f = facts_with(vec![v2(), v16(), phones()]);
            f.mic_permission = permission;
            let r = build_report(&f);
            let m = find_item(&r, SetupItemId::Mic);
            assert_eq!(
                (m.state, m.fix),
                (SetupState::Missing, Some(SetupFix::ConnectMicrophone))
            );
        }
    }

    #[test]
    fn mic_with_unknown_permission_is_error_with_check_again() {
        let mut f = all_ok();
        f.mic_permission = MicPermission::Unknown;
        let r = build_report(&f);
        let m = find_item(&r, SetupItemId::Mic);
        assert_eq!(
            (m.state, m.fix),
            (SetupState::Error, Some(SetupFix::CheckAgain))
        );
    }

    #[test]
    fn mic_skips_a_virtual_default_input_and_names_the_second_input() {
        let virtual_default = v2().with_defaults(true, false);
        let other = dev("Other Mic", DeviceKind::Microphone, 1, 0, Transport::Usb);
        let r = build_report(&facts_with(vec![virtual_default, v16(), other, phones()]));
        let m = find_item(&r, SetupItemId::Mic);
        assert_eq!(m.state, SetupState::Ok);
        assert!(m.detail.contains("Other Mic"), "{}", m.detail);
    }

    #[test]
    fn a_headset_with_an_input_can_be_the_microphone() {
        let headset = dev(
            "Headset",
            DeviceKind::Headphones,
            1,
            2,
            Transport::Bluetooth,
        )
        .with_defaults(true, true);
        let r = build_report(&facts_with(vec![headset, v2(), v16()]));
        let m = find_item(&r, SetupItemId::Mic);
        assert_eq!(m.state, SetupState::Ok);
        assert!(m.detail.contains("Headset"));
    }

    #[test]
    fn capture_uses_the_tap_with_or_without_the_2ch_device() {
        for devices in [vec![mic(), v16(), phones()], all_ok().devices.devices] {
            let r = build_report(&facts_with(devices));
            let c = find_item(&r, SetupItemId::Capture);
            assert_eq!((c.state, c.fix), (SetupState::Ok, None));
            assert!(c.detail.to_lowercase().contains("tap"), "{}", c.detail);
        }
    }

    #[test]
    fn capture_falls_back_to_the_2ch_device_when_the_tap_is_not_available() {
        let mut f = all_ok();
        f.tap_available = false;
        let r = build_report(&f);
        let c = find_item(&r, SetupItemId::Capture);
        assert_eq!((c.state, c.fix), (SetupState::Ok, None));
        assert!(c.detail.contains("2-channel"), "{}", c.detail);
    }

    #[test]
    fn capture_is_missing_without_a_tap_and_without_the_2ch_device() {
        let mut f = facts_with(vec![mic(), v16(), phones()]);
        f.tap_available = false;
        let r = build_report(&f);
        let c = find_item(&r, SetupItemId::Capture);
        assert_eq!(
            (c.state, c.fix),
            (SetupState::Missing, Some(SetupFix::InstallCaptureDevice))
        );
    }

    #[test]
    fn capture_ignores_a_virtual_device_that_is_not_2ch() {
        let odd = dev("Odd", DeviceKind::Other, 2, 0, Transport::Virtual);
        let mut f = facts_with(vec![mic(), v16(), odd, phones()]);
        f.tap_available = false;
        let r = build_report(&f);
        assert_eq!(
            find_item(&r, SetupItemId::Capture).state,
            SetupState::Missing
        );
    }

    #[test]
    fn capture_row_agrees_with_capture_plan_for_a_non_empty_app_id() {
        for tap in [true, false] {
            for with_2ch in [true, false] {
                let devices = if with_2ch { vec![v2()] } else { vec![] };
                let mut f = facts_with(devices.clone());
                f.tap_available = tap;
                let row_ok =
                    find_item(&build_report(&f), SetupItemId::Capture).state == SetupState::Ok;
                let plan_ok = crate::capture_plan(tap, "meeting.app", devices.first()).is_ok();
                assert_eq!(row_ok, plan_ok, "tap {tap}, 2ch {with_2ch}");
            }
        }
    }

    #[test]
    fn capture_row_with_an_empty_app_id_still_counts_the_tap() {
        let mut f = facts_with(vec![]);
        f.tap_available = true;
        assert_eq!(
            find_item(&build_report(&f), SetupItemId::Capture).state,
            SetupState::Ok
        );
    }

    #[test]
    fn vmic_is_ok_with_a_16ch_device_and_missing_without_it() {
        let ok = build_report(&all_ok());
        assert_eq!(find_item(&ok, SetupItemId::Vmic).state, SetupState::Ok);
        let missing = build_report(&facts_with(vec![mic(), v2(), phones()]));
        let v = find_item(&missing, SetupItemId::Vmic);
        assert_eq!(
            (v.state, v.fix),
            (
                SetupState::Missing,
                Some(SetupFix::InstallVirtualMicrophone)
            )
        );
    }

    #[test]
    fn a_16ch_device_alone_does_not_open_the_capture_fallback() {
        let mut f = facts_with(vec![mic(), v16(), phones()]);
        f.tap_available = false;
        let r = build_report(&f);
        assert_eq!(
            find_item(&r, SetupItemId::Capture).state,
            SetupState::Missing
        );
        assert_eq!(find_item(&r, SetupItemId::Vmic).state, SetupState::Ok);
    }

    fn phones_row_for(output: Option<DeviceInfo>) -> SetupItem {
        let mut devices = vec![mic(), v2(), v16()];
        devices.extend(output);
        find_item(&build_report(&facts_with(devices)), SetupItemId::Phones).clone()
    }

    #[test]
    fn phones_is_ok_for_headphones_bluetooth_and_usb_outputs() {
        for transport in [Transport::BuiltIn, Transport::Bluetooth, Transport::Usb] {
            let d = dev("H", DeviceKind::Headphones, 0, 2, transport).with_defaults(false, true);
            let r = phones_row_for(Some(d));
            assert_eq!((r.state, r.fix), (SetupState::Ok, None), "{transport:?}");
        }
    }

    #[test]
    fn phones_is_missing_for_speakers_a_display_a_virtual_output_and_no_output() {
        let speakers = dev("S", DeviceKind::BuiltInSpeakers, 0, 2, Transport::BuiltIn)
            .with_defaults(false, true);
        let display =
            dev("D", DeviceKind::Other, 0, 2, Transport::Display).with_defaults(false, true);
        let virtual_out = v16().with_defaults(false, true);
        for output in [Some(speakers), Some(display), Some(virtual_out), None] {
            let r = phones_row_for(output);
            assert_eq!(
                (r.state, r.fix),
                (SetupState::Missing, Some(SetupFix::ConnectHeadphones))
            );
        }
    }

    #[test]
    fn phones_ignores_a_headphone_device_that_is_not_the_default_output() {
        let not_default = dev("H", DeviceKind::Headphones, 0, 2, Transport::Usb);
        assert_eq!(phones_row_for(Some(not_default)).state, SetupState::Missing);
    }

    #[test]
    fn bluetooth_output_warns_and_other_outputs_do_not() {
        let bt =
            dev("B", DeviceKind::Headphones, 1, 2, Transport::Bluetooth).with_defaults(false, true);
        let r = phones_row_for(Some(bt));
        assert_eq!(
            (r.state, r.warning),
            (SetupState::Ok, Some(SetupWarning::BluetoothOutput))
        );
        let usb = dev("U", DeviceKind::Headphones, 0, 2, Transport::Usb).with_defaults(false, true);
        assert_eq!(phones_row_for(Some(usb)).warning, None);
    }

    #[test]
    fn a_bluetooth_input_with_a_wired_output_gives_no_warning() {
        let bt_in = dev("BI", DeviceKind::Headphones, 1, 2, Transport::Bluetooth);
        let mut f = all_ok();
        f.devices.devices.push(bt_in);
        assert_eq!(
            find_item(&build_report(&f), SetupItemId::Phones).warning,
            None
        );
    }

    #[test]
    fn model_rows_map_all_four_states_in_order() {
        let mut f = all_ok();
        f.models = [
            ModelState::Ok,
            ModelState::Missing,
            ModelState::Downloading,
            ModelState::Error,
        ];
        let r = build_report(&f);
        let got: Vec<(SetupItemId, SetupState, Option<SetupFix>)> = r.items[4..]
            .iter()
            .map(|x| (x.id, x.state, x.fix))
            .collect();
        assert_eq!(
            got,
            [
                (SetupItemId::Sr, SetupState::Ok, None),
                (
                    SetupItemId::Tr,
                    SetupState::Missing,
                    Some(SetupFix::DownloadModel)
                ),
                (
                    SetupItemId::EnV,
                    SetupState::Installing,
                    Some(SetupFix::WaitForInstall)
                ),
                (
                    SetupItemId::ViV,
                    SetupState::Error,
                    Some(SetupFix::DownloadModel)
                ),
            ]
        );
    }

    #[test]
    fn a_row_that_is_not_ok_always_has_a_fix_and_an_ok_row_never() {
        let mut f = facts_with(vec![]);
        f.tap_available = false;
        f.mic_permission = MicPermission::Unknown;
        f.models = [
            ModelState::Missing,
            ModelState::Downloading,
            ModelState::Error,
            ModelState::Ok,
        ];
        for r in build_report(&f).items {
            assert_eq!(r.fix.is_some(), r.state != SetupState::Ok, "{:?}", r.id);
        }
    }

    #[test]
    fn report_json_uses_camel_case_ids_and_has_no_required_field() {
        let json = serde_json::to_value(build_report(&all_ok())).unwrap();
        let ids: Vec<&str> = json["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            ["mic", "capture", "vmic", "phones", "sr", "tr", "enV", "viV"]
        );
        let keys = |v: &serde_json::Value| -> Vec<String> {
            v.as_object().unwrap().keys().cloned().collect()
        };
        assert_eq!(keys(&json), ["items"]);
        assert_eq!(keys(&json["items"][0]), ["detail", "id", "state"]);
    }

    #[test]
    fn permission_denied_error_maps_to_denied_and_the_mic_fix() {
        let permission = mic_permission_from_error(InputOpenError::PermissionDenied);
        assert_eq!(permission, MicPermission::Denied);
        let mut f = all_ok();
        f.mic_permission = permission;
        let r = build_report(&f);
        let m = find_item(&r, SetupItemId::Mic);
        assert_eq!(
            (m.state, m.fix),
            (SetupState::Missing, Some(SetupFix::AllowMicrophone))
        );
    }

    #[test]
    fn other_open_errors_map_to_unknown_not_denied() {
        assert_eq!(
            mic_permission_from_error(InputOpenError::DeviceNotFound),
            MicPermission::Unknown
        );
        assert_eq!(
            mic_permission_from_error(InputOpenError::Other),
            MicPermission::Unknown
        );
    }
}
