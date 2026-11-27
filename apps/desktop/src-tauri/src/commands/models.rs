use meeting_core::egress::{FetchError, FetchRequest, FetchResponse, HttpFetch};
use meeting_core::policy::PrivacyPolicy;
use model_store::{
    CancelToken, DownloadRegistry, ModelEntry, ModelError, ModelSpec, OwnerConfirmation,
};
use std::path::{Path, PathBuf};
use tauri::Manager;

use crate::commands::meeting;
use crate::ipc::{
    CancelModelDownloadArgs, DownloadModelArgs, ErrorCode, IpcError, IpcResult, ModelInfo,
    ModelState,
};
use crate::native_confirm::{self, ConfirmPrompt};

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

/// Show `downloading` for every model whose pack has a download running.
pub fn mark_running_downloads(
    mut models: Vec<ModelInfo>,
    registry: &DownloadRegistry,
) -> Vec<ModelInfo> {
    for model in &mut models {
        let running =
            model_store::entry_for(&model.id).is_some_and(|entry| registry.is_running(entry.pack));
        if running {
            model.state = ModelState::Downloading;
        }
    }
    models
}

/// The list the webview gets: the current rows, with the running downloads on top.
pub fn listed_models(registry: &DownloadRegistry) -> IpcResult<Vec<ModelInfo>> {
    Ok(mark_running_downloads(current_models()?, registry))
}

#[tauri::command]
pub async fn list_models(
    registry: tauri::State<'_, DownloadRegistry>,
) -> IpcResult<Vec<ModelInfo>> {
    listed_models(&registry)
}

/// A model id is an exact id of the compiled-in table (ADR 0012, D-52). The
/// webview can send no url, host or path: an id that is not in the table, in
/// whatever form, is unknown.
fn require_in<'a>(table: &'a [ModelEntry], id: &str) -> IpcResult<&'a ModelEntry> {
    model_store::entry_in(table, id)
        .ok_or_else(|| IpcError::invalid_input(format!("Unknown model: {id}.")))
}

fn require_entry(id: &str) -> IpcResult<&'static ModelEntry> {
    require_in(model_store::entries(), id)
}

/// The refusals that need no dialog and no disk, in order: unknown id
/// (`INVALID_INPUT`), then licence not accepted (`INVALID_INPUT`).
///
/// `licence_accepted` comes from the webview, so it is not proof that the owner
/// clicked (ADR 0006). The proof is an `OwnerConfirmation`, which only the
/// shell's native dialog can make. The flag is read per call and never stored.
pub fn check_download_request(id: &str, licence_accepted: bool) -> IpcResult<&'static ModelEntry> {
    let entry = require_entry(id)?;
    if !licence_accepted {
        return Err(IpcError::invalid_input("Accept the licence first."));
    }
    Ok(entry)
}

/// The models folder (D-6): `<home>/Library/Application Support/Interlo/models`.
/// M1-42 owns the standard storage path and replaces this.
pub fn models_dir_in(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Interlo/models")
}

/// The folder for a given `HOME` value. A missing or relative value gives
/// `None`: a relative path would put the models in whatever folder the app
/// started in.
fn models_dir_from_home(home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let home = PathBuf::from(home?);
    home.is_absolute().then(|| models_dir_in(&home))
}

fn models_dir() -> Option<PathBuf> {
    models_dir_from_home(std::env::var_os("HOME"))
}

/// The policy a model download runs under: local-only mode, and the model hosts
/// of the compiled-in table (`PINNED_HOSTS`) on the allow list, nothing else.
pub fn download_policy() -> PrivacyPolicy {
    let mut policy = PrivacyPolicy::local_only();
    for host in model_store::pinned_hosts() {
        // A host that the policy rejects stays off the list, so its download
        // fails closed; the test `the_download_policy_allows_every_pinned_host_and_no_other`
        // pins the table.
        let _ = policy.allow_model_host(host);
    }
    policy
}

/// What one download hands to the transfer step.
pub struct TransferJob {
    pub entry: &'static ModelEntry,
    pub confirmation: OwnerConfirmation,
    pub token: CancelToken,
    /// `<models folder>/<pack>`: the folder the pack's files and `.part` files use.
    pub pack_dir: PathBuf,
}

/// The transfer step of the shell: the model-store downloader. In M1-04.2 the
/// downloader only runs its gate and stops with `NotImplemented`; M1-04.4 fills
/// it and walks every file of the pack (here: the first file, for the gate).
pub fn store_transfer<'a>(
    fetch: &'a dyn HttpFetch,
    policy: &'a PrivacyPolicy,
) -> impl FnOnce(TransferJob) -> Result<(), ModelError> + 'a {
    move |job| {
        let pack = model_store::pack_of(job.entry);
        let file = pack
            .files
            .first()
            .ok_or(ModelError::NotImplemented("a pack with no files"))?;
        let spec = ModelSpec {
            id: job.entry.id.to_string(),
            url: file.url.to_string(),
            sha256_hex: file.sha256_hex.to_string(),
            size_cap_bytes: file.size_bytes,
        };
        model_store::download(&spec, Some(job.confirmation), policy, fetch, &job.pack_dir)
    }
}

/// The fetch of the shell until M1-04.4 builds the real client: it never
/// reaches a network.
struct UnavailableFetch;

impl HttpFetch for UnavailableFetch {
    fn get(&self, _request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        Err(FetchError::Connection)
    }
}

/// `MEETING_RUNNING` while a meeting runs or starts: nothing is downloaded then.
fn refuse_during_meeting(meeting_running: &dyn Fn() -> bool) -> IpcResult<()> {
    if meeting_running() {
        return Err(IpcError::new(
            ErrorCode::MeetingRunning,
            "A meeting is running.",
        ));
    }
    Ok(())
}

/// The whole `download_model` flow. Refusals run in this order: unknown id,
/// licence not accepted (`INVALID_INPUT`), a meeting (`MEETING_RUNNING`), a
/// download of the pack already running (`INVALID_INPUT`); only then the dialog.
/// A meeting that began while the dialog was open refuses the confirmed download.
/// The pack is registered before the dialog opens, so a second request for the
/// same pack is refused while the first dialog is up, and the registry lock is
/// never held while it is open.
///
/// A declined dialog returns Ok and changes nothing (D-3). A cancel (during the
/// dialog or the transfer) also returns Ok and leaves no `.part` file.
pub fn request_download_with(
    id: &str,
    licence_accepted: bool,
    registry: &DownloadRegistry,
    models_dir: &Path,
    meeting_running: &dyn Fn() -> bool,
    confirm: impl FnOnce(&ConfirmPrompt) -> Option<OwnerConfirmation>,
    transfer: impl FnOnce(TransferJob) -> Result<(), ModelError>,
) -> IpcResult<()> {
    let entry = check_download_request(id, licence_accepted)?;
    refuse_during_meeting(meeting_running)?;
    let guard = registry
        .begin(entry.pack)
        .map_err(|_| IpcError::invalid_input("Already downloading."))?;
    let Some(confirmation) = confirm(&native_confirm::prompt_for(entry)) else {
        return Ok(());
    };
    if guard.token().is_cancelled() {
        return Ok(());
    }
    refuse_during_meeting(meeting_running)?;
    let pack_dir = models_dir.join(entry.pack);
    let job = TransferJob {
        entry,
        confirmation,
        token: guard.token().clone(),
        pack_dir: pack_dir.clone(),
    };
    match transfer(job) {
        Ok(()) => Ok(()),
        Err(ModelError::Cancelled) => {
            // A leftover partial file is removed at the next start of the pack
            // (M1-04.4), so a failed cleanup does not turn a cancel into an error.
            let _ = model_store::remove_partial_files(&pack_dir);
            Ok(())
        }
        // M1-04.4 maps the failures of a real download to `MODEL_ERROR`.
        Err(_) => Err(IpcError::not_implemented("Model download")),
    }
}

/// `request_download_with` with the native dialog, the model-store downloader and
/// the models folder of the app.
pub fn request_download(
    app: &tauri::AppHandle,
    registry: &DownloadRegistry,
    id: &str,
    licence_accepted: bool,
) -> IpcResult<()> {
    // Refuse a bad request before anything reads the home folder.
    check_download_request(id, licence_accepted)?;
    let Some(dir) = models_dir() else {
        return Err(IpcError::not_implemented("Model download"));
    };
    // The policy is read after the dialog, so the transfer sees the session
    // status of that moment.
    request_download_with(
        id,
        licence_accepted,
        registry,
        &dir,
        &|| meeting::meeting_running(app),
        |prompt| native_confirm::confirm_download(app, prompt),
        |job| store_transfer(&UnavailableFetch, &meeting::policy(app))(job),
    )
}

/// Unknown id: `INVALID_INPUT`. Known id: ask the running download of its pack to
/// stop, and return Ok whether one runs or not.
pub fn request_cancel_download(id: &str, registry: &DownloadRegistry) -> IpcResult<()> {
    let entry = require_entry(id)?;
    registry.cancel(entry.pack);
    Ok(())
}

/// Waits for the dialog and the whole download (D-2), so it runs on a blocking
/// thread and never holds a lock that `cancel_model_download` needs.
#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    id: String,
    licence_accepted: bool,
) -> IpcResult<()> {
    let args = DownloadModelArgs {
        id,
        licence_accepted,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let registry = app.state::<DownloadRegistry>();
        request_download(&app, &registry, &args.id, args.licence_accepted)
    })
    .await
    // The task panicked: nothing finished, so report the download as unavailable.
    .unwrap_or_else(|_| Err(IpcError::not_implemented("Model download")))
}

#[tauri::command]
pub async fn cancel_model_download(
    registry: tauri::State<'_, DownloadRegistry>,
    id: String,
) -> IpcResult<()> {
    let args = CancelModelDownloadArgs { id };
    request_cancel_download(&args.id, &registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::setup::sample_setup_status;
    use crate::ipc::{SetupGroup, SetupItemId};
    use std::cell::{Cell, RefCell};

    const IDS: [&str; 4] = ["sr", "tr", "enV", "viV"];

    /// A fetch that counts its calls and always fails.
    #[derive(Default)]
    struct CountingFetch(Cell<u32>);

    impl HttpFetch for CountingFetch {
        fn get(&self, _request: &FetchRequest) -> Result<FetchResponse, FetchError> {
            self.0.set(self.0.get() + 1);
            Err(FetchError::Connection)
        }
    }

    /// A folder in the temp dir that is removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("interlo-models-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn is_empty(&self) -> bool {
            std::fs::read_dir(&self.0).unwrap().next().is_none()
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Everything one download request needs, with a fake dialog that counts.
    struct Env {
        registry: DownloadRegistry,
        dir: TempDir,
        fetch: CountingFetch,
        policy: PrivacyPolicy,
        asked: Cell<u32>,
        prompts: RefCell<Vec<native_confirm::ConfirmPrompt>>,
    }

    impl Env {
        fn new() -> Self {
            Self {
                registry: DownloadRegistry::default(),
                dir: TempDir::new(),
                fetch: CountingFetch::default(),
                policy: download_policy(),
                asked: Cell::new(0),
                prompts: RefCell::new(Vec::new()),
            }
        }

        /// One `download_model` call; `owner_confirms` is what the dialog answers.
        fn download(&self, id: &str, accepted: bool, owner_confirms: bool) -> IpcResult<()> {
            request_download_with(
                id,
                accepted,
                &self.registry,
                self.dir.path(),
                &|| false,
                |prompt| {
                    self.asked.set(self.asked.get() + 1);
                    self.prompts.borrow_mut().push(prompt.clone());
                    owner_confirms.then(|| native_confirm::confirmation_for_tests(&prompt.model_id))
                },
                store_transfer(&self.fetch, &self.policy),
            )
        }

        fn state_of(&self, id: &str) -> ModelState {
            listed_models(&self.registry)
                .unwrap()
                .into_iter()
                .find(|m| m.id == id)
                .unwrap()
                .state
        }
    }

    #[test]
    fn model_info_uses_camel_case_names() {
        let json = serde_json::to_value(&sample_models()[0]).unwrap();
        assert_eq!(json["sizeBytes"], 244_000_000u64);
        assert_eq!(json["state"], "ok");
    }

    // ---- the dialog is asked once, after the checks ----

    #[test]
    fn the_dialog_is_asked_once_after_id_and_licence_with_name_host_and_size() {
        let env = Env::new();
        assert!(env.download("nope", true, true).is_err());
        assert!(env.download("sr", false, true).is_err());
        assert_eq!(env.asked.get(), 0, "no dialog for a refused request");
        let _ = env.download("enV", true, true);
        assert_eq!(env.asked.get(), 1, "exactly one dialog");
        let prompts = env.prompts.borrow();
        let prompt = &prompts[0];
        let entry = model_store::entry_for("enV").unwrap();
        let pack = model_store::pack_of(entry);
        assert_eq!(prompt.model_id, "enV");
        assert_eq!(prompt.title, "Download this model?");
        assert!(prompt.message.contains(entry.name), "{}", prompt.message);
        for host in model_store::pack_hosts(pack) {
            assert!(prompt.message.contains(&host), "{}", prompt.message);
        }
        assert!(
            prompt
                .message
                .contains(&native_confirm::format_size(model_store::pack_size_bytes(
                    pack
                ))),
            "{}",
            prompt.message
        );
    }

    // ---- a declined dialog changes nothing ----

    #[test]
    fn a_declined_dialog_makes_no_fetch_call_and_changes_nothing() {
        let env = Env::new();
        assert_eq!(env.download("enV", true, false), Ok(()));
        assert_eq!(env.asked.get(), 1);
        assert_eq!(env.fetch.0.get(), 0, "no fetch call");
        assert!(env.dir.is_empty(), "no file");
        assert_eq!(env.state_of("enV"), ModelState::Missing);
        assert!(!env.registry.is_running("voice"), "the pack is free again");
    }

    // ---- the webview flag is not a confirmation ----

    #[test]
    fn the_licence_flag_from_the_webview_is_not_a_confirmation() {
        for id in IDS {
            let env = Env::new();
            assert_eq!(env.download(id, true, false), Ok(()), "id {id:?}");
            assert_eq!(env.fetch.0.get(), 0, "id {id:?}: no fetch call");
        }
    }

    #[test]
    fn a_confirmed_download_still_stops_at_the_missing_downloader() {
        for id in IDS {
            let env = Env::new();
            let err = env.download(id, true, true).unwrap_err();
            assert_eq!(err.code, ErrorCode::NotImplemented, "id {id:?}");
            let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/ipc/ipc_error_not_implemented.json");
            let expected: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(&err).unwrap(), expected, "id {id:?}");
            assert_eq!(env.fetch.0.get(), 0, "the stub makes no fetch call");
            assert!(!env.registry.is_running("voice"), "id {id:?}: pack freed");
        }
    }

    // ---- cancel ----

    #[test]
    fn cancel_stops_and_removes_the_partial_file() {
        let registry = DownloadRegistry::default();
        let dir = TempDir::new();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let result = std::thread::scope(|scope| {
            // Owned by this closure, so a failed assertion below drops the sender,
            // the transfer wakes with an error and the scope ends instead of hanging.
            let release_tx = release_tx;
            let download = scope.spawn(|| {
                request_download_with(
                    "enV",
                    true,
                    &registry,
                    dir.path(),
                    &|| false,
                    |prompt| Some(native_confirm::confirmation_for_tests(&prompt.model_id)),
                    move |job: TransferJob| {
                        std::fs::create_dir_all(&job.pack_dir).unwrap();
                        std::fs::write(job.pack_dir.join("voice.bin.part"), b"half").unwrap();
                        started_tx.send(()).unwrap();
                        // The test releases the transfer after it sent the cancel. A cancel
                        // that never reached the flag ends the test in failure, not in a hang.
                        release_rx.recv().unwrap();
                        if job.token.is_cancelled() {
                            Err(ModelError::Cancelled)
                        } else {
                            Err(ModelError::NotImplemented("the cancel never arrived"))
                        }
                    },
                )
            });
            started_rx.recv().expect("the transfer started");
            let shown = mark_running_downloads(sample_models(), &registry);
            for id in ["enV", "viV"] {
                let model = shown.iter().find(|m| m.id == id).unwrap();
                assert_eq!(model.state, ModelState::Downloading, "{id}");
            }
            // The other id of the same pack cancels the transfer too.
            assert_eq!(request_cancel_download("viV", &registry), Ok(()));
            release_tx.send(()).unwrap();
            download.join().unwrap()
        });
        assert_eq!(result, Ok(()), "the waiting download returns Ok");
        assert!(
            !dir.path().join("voice/voice.bin.part").exists(),
            "the partial file is removed"
        );
        assert!(!registry.is_running("voice"));
        let shown = mark_running_downloads(sample_models(), &registry);
        let en = shown.iter().find(|m| m.id == "enV").unwrap();
        assert_eq!(en.state, ModelState::Missing);
    }

    #[test]
    fn cancel_edge_cases() {
        let registry = DownloadRegistry::default();
        for id in [
            "",
            "SR",
            "env",
            "mic",
            "sr ",
            "https://models.example.com/x.bin",
        ] {
            let err = request_cancel_download(id, &registry).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
            assert_eq!(err.message, format!("Unknown model: {id}."));
        }
        for id in IDS {
            assert_eq!(request_cancel_download(id, &registry), Ok(()), "id {id:?}");
        }
        // A cancel with nothing running does not poison the next download.
        let guard = registry.begin("voice").unwrap();
        assert!(!guard.token().is_cancelled());
    }

    #[test]
    fn a_cancel_while_the_dialog_is_open_neither_blocks_nor_starts_the_transfer() {
        let env = Env::new();
        let result = request_download_with(
            "enV",
            true,
            &env.registry,
            env.dir.path(),
            &|| false,
            |prompt| {
                // The registry is not locked while the dialog is open.
                assert!(env.registry.is_running("voice"));
                assert_eq!(request_cancel_download("enV", &env.registry), Ok(()));
                Some(native_confirm::confirmation_for_tests(&prompt.model_id))
            },
            |_job: TransferJob| panic!("a cancelled download must not start"),
        );
        assert_eq!(result, Ok(()));
        assert!(!env.registry.is_running("voice"));
    }

    // ---- one download per pack ----

    #[test]
    fn a_second_download_of_the_pack_is_refused() {
        let env = Env::new();
        let _running = env.registry.begin("voice").unwrap();
        for id in ["enV", "viV"] {
            let err = env.download(id, true, true).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
            assert_eq!(err.message, "Already downloading.");
        }
        assert_eq!(env.asked.get(), 0, "no dialog for a pack that runs");
        for id in ["enV", "viV"] {
            assert_eq!(env.state_of(id), ModelState::Downloading, "{id}");
        }
        assert_ne!(env.state_of("sr"), ModelState::Downloading);
        // Another pack is not blocked.
        assert_eq!(env.download("sr", true, false), Ok(()));
        assert_eq!(env.asked.get(), 1);
    }

    #[test]
    fn the_unknown_id_and_the_licence_come_before_the_running_check() {
        let env = Env::new();
        let _running = env.registry.begin("voice").unwrap();
        assert_eq!(
            env.download("nope", true, true).unwrap_err().message,
            "Unknown model: nope."
        );
        assert_eq!(
            env.download("enV", false, true).unwrap_err().message,
            "Accept the licence first."
        );
    }

    // ---- the licence flag ----

    #[test]
    fn download_without_licence_is_invalid_input_and_asks_nothing() {
        for id in IDS {
            let env = Env::new();
            let err = env.download(id, false, true).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput);
            assert_eq!(err.message, "Accept the licence first.");
            assert_eq!(env.asked.get(), 0, "id {id:?}");
            assert_eq!(env.fetch.0.get(), 0, "id {id:?}");
        }
    }

    #[test]
    fn download_of_an_unknown_id_is_invalid_input() {
        for id in ["", "SR", "env", "mic", "capture", "vmic", "sr-small", "sr "] {
            for accepted in [true, false] {
                let env = Env::new();
                let err = env.download(id, accepted, true).unwrap_err();
                assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
                assert_eq!(err.message, format!("Unknown model: {id}."), "id {id:?}");
                assert_eq!(env.asked.get(), 0, "id {id:?}");
            }
        }
    }

    #[test]
    fn list_models_carries_the_table_licence_and_only_tr_is_non_commercial() {
        let env = Env::new();
        let listed = listed_models(&env.registry).unwrap();
        assert_eq!(listed.len(), 4);
        for model in &listed {
            let entry = model_store::entry_for(&model.id).unwrap();
            assert_eq!(model.licence, entry.licence, "{}", model.id);
            assert_eq!(model.non_commercial, entry.non_commercial, "{}", model.id);
            assert_eq!(model.non_commercial, model.id == "tr", "{}", model.id);
        }
    }

    #[test]
    fn acceptance_is_never_stored() {
        let env = Env::new();
        assert_eq!(env.download("enV", true, false), Ok(()));
        let err = env.download("enV", false, true).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert_eq!(err.message, "Accept the licence first.");
        assert_eq!(env.asked.get(), 1, "the second call asked nothing");
    }

    #[test]
    fn the_check_of_a_request_returns_the_entry_of_the_table() {
        assert_eq!(check_download_request("sr", true).unwrap().id, "sr");
        assert_eq!(
            check_download_request("sr", false).unwrap_err().message,
            "Accept the licence first."
        );
        assert_eq!(
            check_download_request("x", false).unwrap_err().message,
            "Unknown model: x."
        );
    }

    // ---- the downloader inputs ----

    #[test]
    fn the_download_policy_allows_every_pinned_host_and_no_other() {
        let policy = download_policy();
        for host in model_store::pinned_hosts() {
            assert!(policy.check_model_download(host).is_ok(), "{host}");
        }
        assert!(policy
            .check_model_download("elsewhere.example.org")
            .is_err());
    }

    #[test]
    fn the_models_folder_is_under_the_home_application_support_folder() {
        assert_eq!(
            models_dir_in(Path::new("/Users/someone")),
            Path::new("/Users/someone/Library/Application Support/Interlo/models")
        );
    }

    #[test]
    fn only_an_absolute_home_gives_a_models_folder() {
        use std::ffi::OsString;
        assert_eq!(
            models_dir_from_home(Some(OsString::from("/Users/someone"))),
            Some(PathBuf::from(
                "/Users/someone/Library/Application Support/Interlo/models"
            ))
        );
        assert_eq!(
            models_dir_from_home(Some(OsString::from("relative/home"))),
            None
        );
        assert_eq!(models_dir_from_home(Some(OsString::new())), None);
        assert_eq!(models_dir_from_home(None), None);
    }

    #[test]
    fn the_transfer_gets_the_pack_folder_under_the_models_folder() {
        let env = Env::new();
        let seen: RefCell<Option<(PathBuf, String)>> = RefCell::new(None);
        let _ = request_download_with(
            "viV",
            true,
            &env.registry,
            env.dir.path(),
            &|| false,
            |prompt| Some(native_confirm::confirmation_for_tests(&prompt.model_id)),
            |job: TransferJob| {
                *seen.borrow_mut() = Some((
                    job.pack_dir.clone(),
                    job.confirmation.model_id().to_string(),
                ));
                Err(ModelError::NotImplemented("test"))
            },
        );
        let (dir, confirmed) = seen.take().unwrap();
        assert_eq!(dir, env.dir.path().join("voice"));
        assert_eq!(confirmed, "viV");
    }

    #[test]
    fn a_url_as_the_model_id_is_invalid_input_and_asks_nothing() {
        let env = Env::new();
        for id in [
            "https://models.example.com/x/y/model.onnx",
            "http://example.com/model.bin",
            "file:///tmp/model.bin",
            "../sr",
            "sr/../tr",
        ] {
            let err = env.download(id, true, true).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "id {id:?}");
            assert_eq!(err.message, format!("Unknown model: {id}."));
            let err = request_cancel_download(id, &env.registry).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidInput, "cancel id {id:?}");
        }
        assert_eq!(
            env.asked.get(),
            0,
            "no dialog for an id that is not in the table"
        );
        assert_eq!(env.fetch.0.get(), 0);
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
            require_in(&fake, "fake-model").is_ok(),
            "an id of the table is known"
        );
        assert!(
            require_in(&fake, "sr").is_err(),
            "an id that is not in the table is unknown"
        );
        for item in all_setup_items().into_iter().filter(|i| i.is_model()) {
            let json = serde_json::to_value(item).unwrap();
            let id = json.as_str().unwrap();
            assert!(require_entry(id).is_ok(), "{id} has no entry in the table");
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
