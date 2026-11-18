use model_store::OwnerConfirmation;
use serde_json::Value;

#[cfg(any(test, feature = "demo-data"))]
use crate::ipc::ModelState;
use crate::ipc::{
    CancelModelDownloadArgs, DownloadModelArgs, IpcError, IpcResult, ModelInfo, SetupItemId,
};
use crate::native_confirm;

/// Sample rows. Tests and a `demo-data` build only.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_models() -> Vec<ModelInfo> {
    let row = |id: &str, name: &str, size_bytes: u64, state: ModelState, licence: &str| ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        size_bytes,
        state,
        licence: licence.to_string(),
        non_commercial: id == "tr",
    };
    vec![
        row(
            "sr",
            "Speech recognition",
            244_000_000,
            ModelState::Ok,
            "See NOTICE",
        ),
        row(
            "tr",
            "Translation",
            312_000_000,
            ModelState::Ok,
            "CC BY-NC-SA training data",
        ),
        row(
            "enV",
            "English voice",
            63_000_000,
            ModelState::Missing,
            "See NOTICE",
        ),
        row(
            "viV",
            "Vietnamese voice",
            58_000_000,
            ModelState::Ok,
            "See NOTICE",
        ),
    ]
}

/// The model list the owner sees. There is no model table or disk scan yet, so a
/// default build answers `NOT_IMPLEMENTED` instead of inventing rows.
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

/// A model id is the `SetupItemId` of one of the four models, spelled exactly.
fn is_known_model_id(id: &str) -> bool {
    serde_json::from_value::<SetupItemId>(Value::String(id.to_string()))
        .is_ok_and(SetupItemId::is_model)
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
    // The downloader and the compiled-in model table do not exist yet.
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
    use crate::ipc::{ErrorCode, SetupGroup};

    #[test]
    fn model_ids_equal_the_setup_item_ids_of_the_models() {
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
        let models: Vec<String> = sample_models().into_iter().map(|m| m.id).collect();
        assert_eq!(models, setup);
        assert_eq!(models, ["sr", "tr", "enV", "viV"]);
    }

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
    fn sample_models_carry_the_licence_data_of_the_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/ipc/model_info_list.json");
        let fixture_models: Vec<ModelInfo> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let shell: Vec<(String, String, bool)> = sample_models()
            .into_iter()
            .map(|m| (m.id, m.licence, m.non_commercial))
            .collect();
        let expected: Vec<(String, String, bool)> = fixture_models
            .into_iter()
            .map(|m| (m.id, m.licence, m.non_commercial))
            .collect();
        assert_eq!(shell, expected);
    }

    #[test]
    fn only_the_translation_model_is_non_commercial() {
        let flagged: Vec<String> = sample_models()
            .into_iter()
            .filter(|m| m.non_commercial)
            .map(|m| m.id)
            .collect();
        assert_eq!(flagged, ["tr"]);
    }

    #[test]
    fn model_info_serializes_licence_fields_in_camel_case() {
        let json = serde_json::to_value(&sample_models()[1]).unwrap();
        assert_eq!(json["licence"], "CC BY-NC-SA training data");
        assert_eq!(json["nonCommercial"], true);
    }
}
