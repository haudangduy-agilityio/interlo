//! The native confirmation dialog for a model download (ADR 0006).
//!
//! This is the only module that may call `OwnerConfirmation::from_native_dialog`;
//! the test `native_confirm_gate` fails when another shell file does. The owner's
//! click is proven by code the webview cannot call, never by a command argument.

use model_store::OwnerConfirmation;

/// Ask the owner to confirm the download of model `model_id`. `None` means the
/// owner did not confirm, or that no dialog exists yet. There is no dialog yet,
/// so this always returns `None` and every download is refused.
pub fn confirm_download(_model_id: &str) -> Option<OwnerConfirmation> {
    None
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
    fn no_dialog_means_no_confirmation_for_any_model() {
        for id in ["sr", "tr", "enV", "viV", ""] {
            assert!(confirm_download(id).is_none(), "id {id:?}");
        }
    }
}
