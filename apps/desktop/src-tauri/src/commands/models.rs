use model_store::OwnerConfirmation;

#[cfg(any(test, feature = "demo-data"))]
use crate::ipc::ModelState;
use crate::ipc::{CancelModelDownloadArgs, DownloadModelArgs, IpcError, IpcResult, ModelInfo};
use crate::native_confirm;

/// Sample rows. Tests and a `demo-data` build only.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_models() -> Vec<ModelInfo> {
    // Names, licence lines and flags come from the compiled-in table; the sizes
    // and states are sample values.
    model_store::entries()
        .iter()
        .map(|entry| {
            let (size_bytes, state) = match entry.id {
                "sr" => (244_000_000, ModelState::Ok),
                "tr" => (312_000_000, ModelState::Ok),
                "enV" => (63_000_000, ModelState::Missing),
                "viV" => (58_000_000, ModelState::Ok),
                other => unreachable!("sample sizes cover every table id, not {other}"),
            };
            ModelInfo {
                id: entry.id.to_string(),
                name: entry.name.to_string(),
                size_bytes,
                state,
                licence: entry.licence.to_string(),
                non_commercial: entry.non_commercial,
            }
        })
        .collect()
}

/// The model list the owner sees. There is no disk scan yet (the table gives
/// names and licences only), so a default build answers `NOT_IMPLEMENTED`
/// instead of inventing sizes and states.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_models() -> IpcResult<Vec<ModelInfo>> {
    Ok(sample_models())
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn current_models() -> IpcResult<Vec<ModelInfo>> {
    Err(IpcError::not_implemented("Model list"))
}

#[tauri::command]
pub async fn list_models() -> IpcResult<Vec<ModelInfo>> {
    current_models()
}

/// A model id is an exact id of the compiled-in table (ADR 0012, D-52). The
/// webview can send no url, host or path: an id that is not in the table, in
/// whatever form, is unknown.
fn is_known_model_id(id: &str) -> bool {
    is_known_in(model_store::entries(), id)
}

/// The decision of `is_known_model_id` over a given table, so a test can use a
/// table that differs from the Setup enum.
fn is_known_in(table: &[model_store::ModelEntry], id: &str) -> bool {
    model_store::entry_in(table, id).is_some()
}

fn require_known_model_id(id: &str) -> IpcResult<()> {
    if is_known_model_id(id) {
        Ok(())
    } else {
        Err(IpcError::invalid_input(format!("Unknown model: {id}.")))
    }
}

/// Refusals run in this order: unknown id (`INVALID_INPUT`), licence not
/// accepted (`INVALID_INPUT`), then `NOT_IMPLEMENTED` until downloads exist.
///
/// `licence_accepted` comes from the webview, so it is not proof that the owner
/// clicked (ADR 0006). The proof is an `OwnerConfirmation`, which only the
/// shell's native dialog can make.
pub fn request_download(id: &str, licence_accepted: bool) -> IpcResult<()> {
    request_download_with(id, licence_accepted, native_confirm::confirm_download)
}

/// `request_download` with the native dialog passed in, so tests can fake it.
/// `confirm` runs only after the id and the licence flag passed, and at most once.
pub fn request_download_with(
    id: &str,
    licence_accepted: bool,
    confirm: impl FnOnce(&str) -> Option<OwnerConfirmation>,
) -> IpcResult<()> {
    require_known_model_id(id)?;
    if !licence_accepted {
        return Err(IpcError::invalid_input("Accept the licence first."));
    }
    let Some(_confirmation) = confirm(id) else {
        return Err(IpcError::not_implemented("Model download"));
    };
    // The downloader does not exist yet.
    Err(IpcError::not_implemented("Model download"))
}

/// Unknown id: `INVALID_INPUT`. Known id: `NOT_IMPLEMENTED` until downloads exist.
pub fn request_cancel_download(id: &str) -> IpcResult<()> {
    require_known_model_id(id)?;
    Err(IpcError::not_implemented("Cancelling a model download"))
}

#[tauri::command]
pub async fn download_model(id: String, licence_accepted: bool) -> IpcResult<()> {
    let args = DownloadModelArgs {
        id,
        licence_accepted,
    };
    request_download(&args.id, args.licence_accepted)
}

#[tauri::command]
pub async fn cancel_model_download(id: String) -> IpcResult<()> {
    let args = CancelModelDownloadArgs { id };
    request_cancel_download(&args.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_info_uses_camel_case_names() {
        let json = serde_json::to_value(&sample_models()[0]).unwrap();
        assert_eq!(json["sizeBytes"], 244_000_000u64);
        assert_eq!(json["state"], "ok");
    }

    use crate::commands::setup::sample_setup_status;
    use crate::ipc::{ErrorCode, SetupGroup, SetupItemId};

    #[test]
    fn download_of_a_known_model_with_licence_is_not_implemented() {
        for id in ["sr", "tr", "enV", "viV"] {
            assert_eq!(
                request_download(id, true).unwrap_err().code,
                ErrorCode::NotImplemented
            );
        }
    }

    #[test]
    fn download_without_licence_is_invalid_input() {
        for id in ["sr", "tr", "enV", "viV"] {
            let err = request_download(id, false).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput);
            assert_eq!(err.message, "Accept the licence first.");
        }
    }

    #[test]
    fn download_of_an_unknown_id_is_invalid_input() {
        for id in ["", "SR", "env", "mic", "capture", "vmic", "sr-small", "sr "] {
            for accepted in [true, false] {
                let err = request_download(id, accepted).unwrap_err();
                assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
                assert_ne!(err.message, "Accept the licence first.", "id {id:?}");
            }
        }
    }

    #[test]
    fn the_licence_flag_from_the_webview_is_not_a_confirmation() {
        // The real dialog does not exist, so the flag alone never gets past the shell.
        for id in ["sr", "tr", "enV", "viV"] {
            let err = request_download(id, true).unwrap_err();
            assert_eq!(err.code, ErrorCode::NotImplemented, "id {id:?}");
        }
    }

    #[test]
    fn the_dialog_is_asked_only_after_the_id_and_the_licence_pass_and_only_once() {
        use std::cell::Cell;
        let asked = Cell::new(0);
        let ask = |_: &str| {
            asked.set(asked.get() + 1);
            None
        };
        assert!(request_download_with("nope", true, ask).is_err());
        assert!(request_download_with("sr", false, ask).is_err());
        assert_eq!(asked.get(), 0, "no dialog for a refused request");
        assert!(request_download_with("enV", true, ask).is_err());
        assert_eq!(asked.get(), 1);
    }

    #[test]
    fn the_dialog_is_asked_about_the_requested_model() {
        let mut seen = String::new();
        let _ = request_download_with("viV", true, |id| {
            seen = id.to_string();
            Some(native_confirm::confirmation_for_tests(id))
        });
        assert_eq!(seen, "viV");
    }

    #[test]
    fn a_confirmed_download_still_stops_at_the_missing_downloader() {
        let err = request_download_with("sr", true, |id| {
            Some(native_confirm::confirmation_for_tests(id))
        })
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotImplemented);
    }

    #[test]
    fn cancel_of_a_known_model_is_not_implemented() {
        for id in ["sr", "tr", "enV", "viV"] {
            assert_eq!(
                request_cancel_download(id).unwrap_err().code,
                ErrorCode::NotImplemented
            );
        }
    }

    #[test]
    fn cancel_of_an_unknown_id_is_invalid_input() {
        for id in ["", "SR", "env", "mic", "sr "] {
            assert_eq!(
                request_cancel_download(id).unwrap_err().code,
                ErrorCode::InvalidInput,
                "id {id:?}"
            );
        }
    }

    #[test]
    fn the_table_carries_the_licence_data_of_the_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/ipc/model_info_list.json");
        let fixture_models: Vec<ModelInfo> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let shell: Vec<(String, String, bool)> = model_store::entries()
            .iter()
            .map(|e| (e.id.to_string(), e.licence.to_string(), e.non_commercial))
            .collect();
        let expected: Vec<(String, String, bool)> = fixture_models
            .into_iter()
            .map(|m| (m.id, m.licence, m.non_commercial))
            .collect();
        assert_eq!(shell, expected);
    }

    #[test]
    fn model_info_serializes_licence_fields_in_camel_case() {
        let json = serde_json::to_value(&sample_models()[1]).unwrap();
        assert_eq!(
            json["licence"],
            "Apache-2.0. Training data is CC BY-NC-SA: non-commercial use only"
        );
        assert_eq!(json["nonCommercial"], true);
    }

    #[test]
    fn a_url_as_the_model_id_is_invalid_input_and_asks_nothing() {
        use std::cell::Cell;
        let asked = Cell::new(0);
        let ask = |_: &str| {
            asked.set(asked.get() + 1);
            Some(native_confirm::confirmation_for_tests("sr"))
        };
        for id in [
            "https://models.example.com/x/y/model.onnx",
            "http://example.com/model.bin",
            "file:///tmp/model.bin",
            "../sr",
            "sr/../tr",
        ] {
            let err = request_download_with(id, true, ask).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
            assert_eq!(err.message, format!("Unknown model: {id}."));
            let err = request_cancel_download(id).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "cancel id {id:?}");
        }
        assert_eq!(
            asked.get(),
            0,
            "no dialog for an id that is not in the table"
        );
    }

    /// Every variant of `SetupItemId`. The match has no wildcard, so a new variant
    /// does not compile until it is listed here.
    fn all_setup_items() -> [SetupItemId; 8] {
        let all = [
            SetupItemId::Mic,
            SetupItemId::Capture,
            SetupItemId::Vmic,
            SetupItemId::Phones,
            SetupItemId::Sr,
            SetupItemId::Tr,
            SetupItemId::EnV,
            SetupItemId::ViV,
        ];
        for item in all {
            match item {
                SetupItemId::Mic
                | SetupItemId::Capture
                | SetupItemId::Vmic
                | SetupItemId::Phones
                | SetupItemId::Sr
                | SetupItemId::Tr
                | SetupItemId::EnV
                | SetupItemId::ViV => {}
            }
        }
        all
    }

    #[test]
    fn the_id_check_reads_the_table_not_the_setup_enum() {
        let fake = [model_store::ModelEntry {
            id: "fake-model",
            name: "Fake",
            licence: "Test",
            non_commercial: false,
            pack: "none",
        }];
        assert!(
            is_known_in(&fake, "fake-model"),
            "an id of the table is known"
        );
        assert!(
            !is_known_in(&fake, "sr"),
            "an id that is not in the table is unknown"
        );
        for item in all_setup_items().into_iter().filter(|i| i.is_model()) {
            let json = serde_json::to_value(item).unwrap();
            let id = json.as_str().unwrap();
            assert!(is_known_model_id(id), "{id} has no entry in the table");
        }
    }

    #[test]
    fn table_ids_equal_the_models_group_of_setup_item_id() {
        let table: Vec<&str> = model_store::entries().iter().map(|e| e.id).collect();
        let setup: Vec<String> = sample_setup_status()
            .items
            .iter()
            .filter(|i| i.group == SetupGroup::Models)
            .map(|i| {
                serde_json::to_value(i.id)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(table, setup);
        for id in &table {
            let parsed: SetupItemId = serde_json::from_value(serde_json::json!(id)).unwrap();
            assert!(parsed.is_model(), "{id} is not a models-group id");
        }
    }
}
