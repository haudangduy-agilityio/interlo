//! The model folder of the developer build (SL-01). A default build reads none:
//! the name of the environment variable exists only in a build with the feature
//! `dev-models`.

use std::path::PathBuf;

/// Where this program run looks for model files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFolder {
    /// This build reads no model folder: the model rows stay as they are.
    NotRead,
    /// The developer build found no usable folder; the text says so.
    Unusable(&'static str),
    /// An absolute path of an existing folder.
    At(PathBuf),
}

#[cfg(feature = "dev-models")]
const VARIABLE: &str = "INTERLO_DEV_MODELS_DIR";
/// The row detail of a developer build without a usable folder.
#[cfg(feature = "dev-models")]
const NO_FOLDER: &str = "INTERLO_DEV_MODELS_DIR does not name a model folder";

/// The folder of this program run.
#[cfg(not(feature = "dev-models"))]
pub fn from_environment() -> ModelFolder {
    ModelFolder::NotRead
}

/// The folder of this program run.
#[cfg(feature = "dev-models")]
pub fn from_environment() -> ModelFolder {
    folder_named(std::env::var_os(VARIABLE))
}

/// The folder that a value of the variable names: an absolute path of an
/// existing folder.
#[cfg(feature = "dev-models")]
pub(crate) fn folder_named(value: Option<std::ffi::OsString>) -> ModelFolder {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .map_or(ModelFolder::Unusable(NO_FOLDER), ModelFolder::At)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dev-models")]
    use super::folder_named;
    #[cfg(not(feature = "dev-models"))]
    use super::*;
    use crate::ipc::SetupState::Missing;
    use crate::session::testing::{model_rows, model_states, session_with, Folder};

    #[cfg(not(feature = "dev-models"))]
    #[test]
    fn default_build_ignores_the_model_folder_variable() {
        use crate::session::model_check::ModelCheck;
        use crate::session::testing::PACKS;
        use model_store::Sha256Hasher;

        let folder = Folder::new();
        // No other test of this build reads the variable.
        std::env::set_var("INTERLO_DEV_MODELS_DIR", folder.path());
        let check = ModelCheck::launch(
            from_environment(),
            &PACKS,
            folder.uid(),
            Box::new(Sha256Hasher),
        );
        let status = session_with(check).check_setup();
        assert_eq!(model_states(&status), [Missing; 4]);
        assert_eq!(
            model_rows(&status)[0].1,
            "Speech recognition model not downloaded"
        );
    }

    #[cfg(feature = "dev-models")]
    #[test]
    fn no_model_folder_leaves_the_model_rows_missing() {
        use crate::session::model_check::ModelCheck;
        use crate::session::testing::PACKS;
        use model_store::Sha256Hasher;

        let folder = Folder::new();
        let values = [
            None,
            Some("".into()),
            Some("relative/models".into()),
            Some(folder.at("not-there").into_os_string()),
            Some(folder.at("asr/a.bin").into_os_string()),
        ];
        for value in values {
            let check = ModelCheck::launch(
                folder_named(value.clone()),
                &PACKS,
                folder.uid(),
                Box::new(Sha256Hasher),
            );
            let status = session_with(check).check_setup();
            assert_eq!(model_states(&status), [Missing; 4], "{value:?}");
            for row in model_rows(&status) {
                assert_eq!(
                    row.1, "INTERLO_DEV_MODELS_DIR does not name a model folder",
                    "{value:?}"
                );
            }
        }
        // A usable folder is read.
        let check = ModelCheck::launch(
            folder_named(Some(folder.path().into())),
            &PACKS,
            folder.uid(),
            Box::new(Sha256Hasher),
        );
        let status = session_with(check).check_setup();
        assert_eq!(model_states(&status), [crate::ipc::SetupState::Ok; 4]);
    }
}
