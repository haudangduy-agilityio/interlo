use serde_json::Value;

#[cfg(any(test, feature = "demo-data"))]
use crate::ipc::UiLanguage;
use crate::ipc::{AppSettings, IpcError, IpcResult, SetSettingArgs};

/// Settings exposed to the UI. User preferences only; nothing sensitive. Tests
/// and a `demo-data` build only.
#[cfg(any(test, feature = "demo-data"))]
pub fn sample_settings() -> AppSettings {
    AppSettings {
        ui_language: UiLanguage::En,
        ducking: true,
        auto_delete_days: None,
    }
}

/// Check that `key` is known and `value` has the right shape.
pub fn validate_setting(key: &str, value: &Value) -> IpcResult<()> {
    let ok = match key {
        "uiLanguage" => matches!(value.as_str(), Some("en" | "vi")),
        "ducking" => value.is_boolean(),
        "autoDeleteDays" => {
            value.is_null()
                || value
                    .as_u64()
                    .is_some_and(|n| n > 0 && u32::try_from(n).is_ok())
        }
        _ => return Err(IpcError::invalid_input(format!("Unknown setting: {key}."))),
    };
    if ok {
        Ok(())
    } else {
        Err(IpcError::invalid_input(format!(
            "Invalid value for setting: {key}."
        )))
    }
}

/// The settings the owner sees. There is no settings store yet, so a default
/// build answers `NOT_IMPLEMENTED` instead of showing values nobody saved.
#[cfg(any(test, feature = "demo-data"))]
pub fn current_settings() -> IpcResult<AppSettings> {
    Ok(sample_settings())
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn current_settings() -> IpcResult<AppSettings> {
    Err(IpcError::not_implemented("Reading settings"))
}

/// Validate and store one setting. The input is checked first, so bad input is
/// `INVALID_INPUT` in every build. There is no settings store yet, so a default
/// build answers `NOT_IMPLEMENTED` and never `Ok` for a value it did not keep.
#[cfg(any(test, feature = "demo-data"))]
pub fn apply_setting(key: &str, value: &Value) -> IpcResult<()> {
    validate_setting(key, value)
}

/// See the `demo-data` variant above.
#[cfg(not(any(test, feature = "demo-data")))]
pub fn apply_setting(key: &str, value: &Value) -> IpcResult<()> {
    validate_setting(key, value)?;
    Err(IpcError::not_implemented("Saving settings"))
}

#[tauri::command]
pub async fn get_settings() -> IpcResult<AppSettings> {
    current_settings()
}

#[tauri::command]
pub async fn set_setting(key: String, value: Value) -> IpcResult<()> {
    let args = SetSettingArgs { key, value };
    apply_setting(&args.key, &args.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ErrorCode;
    use serde_json::json;

    #[test]
    fn settings_use_camel_case_names() {
        let json = serde_json::to_value(sample_settings()).unwrap();
        assert_eq!(json["uiLanguage"], "en");
        assert_eq!(json["ducking"], true);
        assert!(json["autoDeleteDays"].is_null());
    }

    #[test]
    fn unknown_key_is_invalid_input() {
        let err = validate_setting("apiKey", &json!("x")).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn known_keys_check_value_shape() {
        assert!(validate_setting("uiLanguage", &json!("vi")).is_ok());
        assert!(validate_setting("uiLanguage", &json!("fr")).is_err());
        assert!(validate_setting("ducking", &json!(false)).is_ok());
        assert!(validate_setting("ducking", &json!("yes")).is_err());
        assert!(validate_setting("autoDeleteDays", &json!(30)).is_ok());
        assert!(validate_setting("autoDeleteDays", &Value::Null).is_ok());
        assert!(validate_setting("autoDeleteDays", &json!(-1)).is_err());
    }

    #[test]
    fn auto_delete_days_must_be_null_or_a_positive_integer() {
        assert!(validate_setting("autoDeleteDays", &json!(1)).is_ok());
        assert!(validate_setting("autoDeleteDays", &json!(u32::MAX)).is_ok());
        for bad in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("30"),
            json!(u64::from(u32::MAX) + 1),
        ] {
            let err = validate_setting("autoDeleteDays", &bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "value {bad}");
        }
    }
}
