//! The native confirmation dialog for a model download (ADR 0006).
//!
//! This is the only module that may call `OwnerConfirmation::from_native_dialog`;
//! the test `native_confirm_gate` fails when another shell file does. The owner's
//! click is proven by code the webview cannot call, never by a command argument.

use model_store::{ModelEntry, OwnerConfirmation};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// What the native dialog shows: the question, the model name, where the files
/// come from and how big they are. It holds no url, path or checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmPrompt {
    /// The model the owner is asked about; a confirmation names this id.
    pub model_id: String,
    pub title: String,
    pub message: String,
}

const BYTES_PER_MB: u64 = 1_000_000;
const BYTES_PER_GB: u64 = 1_000_000_000;

/// The prompt for `entry`, built from the compiled-in table only.
pub fn prompt_for(entry: &ModelEntry) -> ConfirmPrompt {
    let pack = model_store::pack_of(entry);
    let hosts = model_store::pack_hosts(pack).join(", ");
    let size = format_size(model_store::pack_size_bytes(pack));
    ConfirmPrompt {
        model_id: entry.id.to_string(),
        title: "Download this model?".to_string(),
        message: format!("{}\nSource: {hosts}\nSize: {size}", entry.name),
    }
}

/// The size the owner reads. It rounds up, so the download is never bigger than
/// the dialog says. Decimal units, as the disk and the network count them.
pub fn format_size(bytes: u64) -> String {
    if bytes >= BYTES_PER_GB {
        let tenths = bytes.div_ceil(BYTES_PER_GB / 10);
        format!("{}.{} GB", tenths / 10, tenths % 10)
    } else {
        format!("{} MB", bytes.div_ceil(BYTES_PER_MB))
    }
}

/// Show the native dialog and wait for the owner's answer. `None` means the owner
/// cancelled or closed the dialog. Call it off the main thread (the download
/// command runs it in a blocking task). Only the confirm button makes a
/// confirmation; the webview has no way to press it.
pub fn confirm_download(
    app: &tauri::AppHandle,
    prompt: &ConfirmPrompt,
) -> Option<OwnerConfirmation> {
    let confirmed = app
        .dialog()
        .message(prompt.message.clone())
        .title(prompt.title.clone())
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Download".to_string(),
            "Cancel".to_string(),
        ))
        .blocking_show();
    confirmed.then(|| OwnerConfirmation::from_native_dialog(&prompt.model_id))
}

/// A confirmation for shell unit tests. It lives here so that no other shell file
/// names `from_native_dialog`; it is compiled out of every non-test build.
#[cfg(test)]
pub(crate) fn confirmation_for_tests(model_id: &str) -> OwnerConfirmation {
    OwnerConfirmation::from_native_dialog(model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_round_up_so_the_owner_never_expects_less_than_the_download() {
        assert_eq!(format_size(63_000_000), "63 MB");
        assert_eq!(format_size(63_000_001), "64 MB");
        assert_eq!(format_size(1_500_000_000), "1.5 GB");
        assert_eq!(format_size(1_500_000_001), "1.6 GB");
        assert_eq!(format_size(1_000_000_000), "1.0 GB");
        assert_eq!(format_size(0), "0 MB");
    }

    #[test]
    fn the_prompt_names_the_model_the_hosts_and_the_size_for_every_entry() {
        for entry in model_store::entries() {
            let pack = model_store::pack_of(entry);
            let prompt = prompt_for(entry);
            assert_eq!(prompt.model_id, entry.id);
            assert_eq!(prompt.title, "Download this model?");
            assert!(prompt.message.contains(entry.name), "{}", prompt.message);
            let hosts = model_store::pack_hosts(pack);
            assert!(!hosts.is_empty());
            for host in hosts {
                assert!(prompt.message.contains(&host), "{}", prompt.message);
            }
            assert!(
                prompt
                    .message
                    .contains(&format_size(model_store::pack_size_bytes(pack))),
                "{}",
                prompt.message
            );
        }
    }

    #[test]
    fn the_prompt_holds_no_url_path_or_checksum() {
        for entry in model_store::entries() {
            let prompt = prompt_for(entry);
            for file in model_store::pack_of(entry).files {
                assert!(!prompt.message.contains(file.url), "{}", prompt.message);
                assert!(!prompt.message.contains(file.sha256_hex));
                assert!(!prompt.message.contains(file.rel_path));
            }
            assert!(!prompt.message.contains("https://"), "{}", prompt.message);
        }
    }
}
