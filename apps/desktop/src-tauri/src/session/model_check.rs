//! The check of the model files: once at app launch (and again before a load),
//! shown in the four model rows of the setup status.

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;

use crate::ipc::{SetupItemId, SetupState, SetupStatus};
use crate::models_dir::ModelFolder;
use model_store::{check_pack, Hasher, Pack, PackCheck};

/// The hasher of the check.
pub type ModelHasher = Box<dyn Hasher + Send + Sync>;

/// The detail of a model row whose files failed the check.
const FILE_FAILED: &str = "A model file failed its check";

/// Each model row, the pack that holds its files and the name the row shows.
const ROWS: [(SetupItemId, &str, &str); 4] = [
    (SetupItemId::Sr, "asr", "Speech recognition model"),
    (SetupItemId::Tr, "translation", "Translation model"),
    (SetupItemId::EnV, "voice", "English voice model"),
    (SetupItemId::ViV, "voice", "Vietnamese voice model"),
];

/// What the check found, per pack.
type Results = Vec<(&'static str, PackCheck)>;

struct Shared {
    folder: ModelFolder,
    packs: &'static [Pack],
    owner_uid: u32,
    hasher: ModelHasher,
    /// `None` until the launch check ends.
    results: Mutex<Option<Results>>,
    ended: Condvar,
}

impl Shared {
    fn check(&self, pack: &Pack) -> PackCheck {
        match &self.folder {
            ModelFolder::At(root) => check_pack(root, pack, self.owner_uid, self.hasher.as_ref()),
            ModelFolder::NotRead | ModelFolder::Unusable(_) => PackCheck::Missing,
        }
    }

    fn finish(&self, results: Results) {
        *self.results.lock().unwrap_or_else(PoisonError::into_inner) = Some(results);
        self.ended.notify_all();
    }

    /// Replace the result of one pack.
    fn keep(&self, pack: &str, found: PackCheck) {
        let mut results = self.results.lock().unwrap_or_else(PoisonError::into_inner);
        let kept = results.iter_mut().flatten();
        for (_, result) in kept.filter(|(checked, _)| *checked == pack) {
            *result = found;
        }
    }

    /// The results, after the launch check has ended.
    fn results(&self) -> Results {
        let mut results = self.results.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(found) = results.as_ref() {
                return found.clone();
            }
            results = self
                .ended
                .wait(results)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// The answer when the launch check cannot run to its end.
    fn all_failed(&self) -> Results {
        self.packs
            .iter()
            .map(|pack| (pack.id, PackCheck::Failed))
            .collect()
    }
}

/// The results of the model checks of one program run.
pub struct ModelCheck {
    shared: Arc<Shared>,
}

impl ModelCheck {
    /// Start the launch check of every pack of `packs` on its own thread.
    /// `owner_uid` is the user that must own each file.
    pub fn launch(
        folder: ModelFolder,
        packs: &'static [Pack],
        owner_uid: u32,
        hasher: ModelHasher,
    ) -> Self {
        let shared = Arc::new(Shared {
            folder,
            packs,
            owner_uid,
            hasher,
            results: Mutex::new(None),
            ended: Condvar::new(),
        });
        if !matches!(shared.folder, ModelFolder::At(_)) {
            shared.finish(Vec::new());
            return Self { shared };
        }
        let worker = Arc::clone(&shared);
        let started = thread::Builder::new()
            .name("model-check".to_string())
            .spawn(move || {
                let all = || {
                    let packs = worker.packs.iter();
                    packs.map(|pack| (pack.id, worker.check(pack))).collect()
                };
                // A check that panics fails every pack; nobody waits for ever.
                let results = panic::catch_unwind(AssertUnwindSafe(all))
                    .unwrap_or_else(|_| worker.all_failed());
                worker.finish(results);
            });
        if started.is_err() {
            shared.finish(shared.all_failed());
        }
        Self { shared }
    }

    /// The model folder, when this build reads one that exists.
    pub fn folder(&self) -> Option<&Path> {
        match &self.shared.folder {
            ModelFolder::At(root) => Some(root),
            ModelFolder::NotRead | ModelFolder::Unusable(_) => None,
        }
    }

    /// The check just before a load (D-179): wait for the launch check, check
    /// the files of one pack again and keep the result for the setup rows.
    pub fn check_again(&self, pack_id: &str) -> PackCheck {
        if self.folder().is_none() {
            return PackCheck::Missing;
        }
        // The launch check ends first, so it cannot overwrite this result.
        self.shared.results();
        let Some(pack) = self.shared.packs.iter().find(|pack| pack.id == pack_id) else {
            return PackCheck::Failed;
        };
        let found = self.shared.check(pack);
        self.shared.keep(pack.id, found);
        found
    }

    /// Set the four model rows of `status`. Waits for the launch check. A build
    /// that reads no model folder leaves `status` as it is.
    pub fn apply(&self, status: &mut SetupStatus) {
        let rows = match &self.shared.folder {
            ModelFolder::NotRead => return,
            ModelFolder::Unusable(why) => {
                ROWS.map(|(id, _, _)| (id, SetupState::Missing, (*why).to_string()))
            }
            ModelFolder::At(_) => {
                let results = self.shared.results();
                ROWS.map(|(id, pack, name)| {
                    let found = results.iter().find(|(checked, _)| *checked == pack);
                    match found.map_or(PackCheck::Failed, |(_, result)| *result) {
                        PackCheck::Ok => (id, SetupState::Ok, name.to_string()),
                        PackCheck::Missing => {
                            (id, SetupState::Missing, format!("{name} not downloaded"))
                        }
                        PackCheck::Failed => (id, SetupState::Error, FILE_FAILED.to_string()),
                    }
                })
            }
        };
        let mut items = std::mem::take(&mut status.items);
        for item in &mut items {
            if let Some((_, state, detail)) = rows.iter().find(|row| row.0 == item.id) {
                item.state = *state;
                item.detail.clone_from(detail);
            }
        }
        *status = SetupStatus::from_items(items);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::SetupState::{Error, Missing, Ok};
    use crate::session::testing::{
        model_rows, model_states, session_with, Folder, Recording, FILES, PACKS,
    };
    use model_store::{FileDigest, Sha256Hasher};
    use std::fs::{self, File};
    use std::io;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const FAILED: &str = "A model file failed its check";

    #[test]
    fn matching_packs_set_the_model_rows_ok() {
        let folder = Folder::new();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Ok, Ok, Ok, Ok]);
        let details: Vec<String> = model_rows(&status).into_iter().map(|row| row.1).collect();
        assert_eq!(
            details,
            [
                "Speech recognition model",
                "Translation model",
                "English voice model",
                "Vietnamese voice model"
            ]
        );
        assert!(status.can_start);
        assert!(status.missing_groups.is_empty());
    }

    #[test]
    fn changed_byte_fails_only_its_pack() {
        let folder = Folder::new();
        fs::write(folder.at("voice/sub/w.bin"), b"abd").unwrap();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Ok, Ok, Error, Error]);
        let rows = model_rows(&status);
        assert_eq!((rows[2].1.as_str(), rows[3].1.as_str()), (FAILED, FAILED));
        assert!(!status.can_start);
        assert_eq!(status.missing_groups, ["Models"]);
    }

    #[test]
    fn missing_file_leaves_its_row_missing() {
        let folder = Folder::new();
        fs::remove_file(folder.at("asr/a.bin")).unwrap();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Missing, Ok, Ok, Ok]);
        assert_eq!(
            model_rows(&status)[0].1,
            "Speech recognition model not downloaded"
        );

        fs::write(folder.at("asr/vad/vad.onnx"), b"abd").unwrap();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Error, Ok, Ok, Ok]);

        let folder = Folder::new();
        let unreadable = folder.at("translation/t.bin");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Ok, Error, Ok, Ok]);
    }

    #[test]
    fn symlinked_file_or_folder_is_refused() {
        let folder = Folder::new();
        let target = folder.at("matching.bin");
        fs::write(&target, b"abc").unwrap();
        fs::remove_file(folder.at("asr/a.bin")).unwrap();
        symlink(&target, folder.at("asr/a.bin")).unwrap();
        fs::rename(folder.at("voice/sub"), folder.at("moved")).unwrap();
        symlink(folder.at("moved"), folder.at("voice/sub")).unwrap();
        let status = session_with(folder.check()).check_setup();
        assert_eq!(model_states(&status), [Error, Ok, Error, Error]);

        let folder = Folder::new();
        let link = folder.at("link-to-the-folder");
        symlink(folder.path(), &link).unwrap();
        let check = ModelCheck::launch(
            ModelFolder::At(link),
            &PACKS,
            folder.uid(),
            Box::new(Sha256Hasher),
        );
        let status = session_with(check).check_setup();
        assert_eq!(model_states(&status), [Ok, Ok, Ok, Ok]);
    }

    #[test]
    fn file_of_another_owner_is_refused() {
        let folder = Folder::new();
        let check = ModelCheck::launch(
            folder.source(),
            &PACKS,
            folder.uid() + 1,
            Box::new(Sha256Hasher),
        );
        let status = session_with(check).check_setup();
        assert_eq!(model_states(&status), [Error, Error, Error, Error]);
    }

    #[test]
    fn launch_check_hashes_each_file_once() {
        let folder = Folder::new();
        let seen = Recording::default();
        let check = ModelCheck::launch(
            folder.source(),
            &PACKS,
            folder.uid(),
            Box::new(seen.clone()),
        );
        // The check runs without a call.
        let started = Instant::now();
        while seen.hashed().len() < FILES.len() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "the launch check did not run"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let session = session_with(check);
        session.check_setup();
        session.check_setup();
        let mut hashed = seen.hashed();
        hashed.sort_unstable();
        let mut files: Vec<u64> = FILES.iter().map(|name| folder.number(name)).collect();
        files.sort_unstable();
        assert_eq!(hashed, files);
    }

    /// Waits for a word before it hashes anything.
    struct Held(Mutex<mpsc::Receiver<()>>);

    impl Hasher for Held {
        fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
            // After the sender is gone, every call goes on.
            let _ = self.0.lock().unwrap().recv();
            Sha256Hasher.digest(file, cap_bytes)
        }
    }

    struct Panics;

    impl Hasher for Panics {
        fn digest(&self, _file: &mut File, _cap_bytes: u64) -> io::Result<FileDigest> {
            panic!("the hasher fails on purpose");
        }
    }

    #[test]
    fn check_setup_waits_for_the_launch_check() {
        let folder = Folder::new();
        fs::write(folder.at("translation/t.bin"), b"abd").unwrap();
        let (release, held) = mpsc::channel();
        let check = ModelCheck::launch(
            folder.source(),
            &PACKS,
            folder.uid(),
            Box::new(Held(Mutex::new(held))),
        );
        let session = session_with(check);
        let (answer, answered) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| answer.send(session.check_setup()).unwrap());
            assert!(
                answered.recv_timeout(Duration::from_millis(200)).is_err(),
                "answered while the check ran"
            );
            drop(release);
            let status = answered.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(model_states(&status), [Ok, Error, Ok, Ok]);
        });

        let check = ModelCheck::launch(folder.source(), &PACKS, folder.uid(), Box::new(Panics));
        let status = session_with(check).check_setup();
        assert_eq!(model_states(&status), [Error, Error, Error, Error]);
        assert!(model_rows(&status).iter().all(|row| row.1 == FAILED));
    }
}
