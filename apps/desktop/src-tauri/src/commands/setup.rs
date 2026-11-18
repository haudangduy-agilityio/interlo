#[cfg(not(any(test, feature = "demo-data")))]
use crate::ipc::IpcError;
use crate::ipc::{IpcResult, SetupStatus};
#[cfg(any(test, feature = "demo-data"))]
use crate::ipc::{SetupGroup, SetupItem, SetupItemId, SetupState};

#[cfg(any(test, feature = "demo-data"))]
fn item(id: SetupItemId, group: SetupGroup, state: SetupState, detail: &str) -> SetupItem {
    SetupItem {
        id,
        group,
        state,
        detail: detail.to_string(),
    }
}

/// Fixed sample: everything ready except the English voice model. Tests and a
/// `demo-data` build only: a default build never reports sample setup data.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_setup_status() -> SetupStatus {
    use SetupGroup::{Devices, Models, VirtualMicrophone};
    use SetupItemId::*;
    use SetupState::{Missing, Ok};
    SetupStatus::from_items(vec![
        item(Mic, Devices, Ok, "Built-in microphone"),
        item(Capture, Devices, Ok, "2-channel virtual device"),
        item(Vmic, VirtualMicrophone, Ok, "16-channel virtual device"),
        item(Phones, Devices, Ok, "Headphones connected"),
        item(Sr, Models, Ok, "Speech recognition model"),
        item(Tr, Models, Ok, "Translation model"),
        item(EnV, Models, Missing, "English voice model not downloaded"),
        item(ViV, Models, Ok, "Vietnamese voice model"),
    ])
}

/// The setup status the owner sees. Device and model detection do not exist yet,
/// so a default build answers `NOT_IMPLEMENTED` instead of inventing a status.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_setup_status() -> IpcResult<SetupStatus> {
    Ok(sample_setup_status())
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn current_setup_status() -> IpcResult<SetupStatus> {
    Err(IpcError::not_implemented("Setup check"))
}

#[tauri::command]
pub async fn check_setup() -> IpcResult<SetupStatus> {
    current_setup_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_status_uses_camel_case_names() {
        let json = serde_json::to_value(sample_setup_status()).unwrap();
        assert_eq!(json["canStart"], false);
        assert_eq!(json["missingGroups"], serde_json::json!(["Models"]));
        assert!(json.get("can_start").is_none());
        let first = &json["items"][0];
        assert_eq!(first["id"], "mic");
        assert_eq!(first["group"], "devices");
        assert_eq!(first["state"], "ok");
        let ids: Vec<&str> = json["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            ["mic", "capture", "vmic", "phones", "sr", "tr", "enV", "viV"]
        );
        assert_eq!(json["items"][2]["group"], "virtualMicrophone");
    }

    #[test]
    fn can_start_when_all_required_items_are_ok() {
        let mut status = sample_setup_status();
        for item in &mut status.items {
            item.state = SetupState::Ok;
        }
        let status = SetupStatus::from_items(status.items);
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }

    #[test]
    fn missing_virtual_microphone_is_named_with_its_display_name() {
        let mut items = sample_setup_status().items;
        for item in &mut items {
            item.state = SetupState::Ok;
        }
        items[2].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Virtual microphone"]);
    }

    #[test]
    fn missing_microphone_or_meeting_audio_blocks_start_with_display_names() {
        let ready = || {
            let mut items = sample_setup_status().items;
            for item in &mut items {
                item.state = SetupState::Ok;
            }
            items
        };
        let mut items = ready();
        items[0].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Microphone"]);

        let mut items = ready();
        items[1].state = SetupState::Error;
        let status = SetupStatus::from_items(items);
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Meeting audio"]);
    }

    #[test]
    fn missing_headphones_do_not_block_start() {
        let mut items = sample_setup_status().items;
        for item in &mut items {
            item.state = SetupState::Ok;
        }
        items[3].state = SetupState::Missing;
        let status = SetupStatus::from_items(items);
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }
}
