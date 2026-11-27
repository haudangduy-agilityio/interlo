//! The speech engines of the app: loaded at the first Start from files that pass
//! their check, kept for later sessions, and dropped after a failure.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use engine_host::live_chain::Engines;
use model_store::PackCheck;

use crate::session::flight::Flight;
use crate::session::machine::Machine;
use crate::session::model_check::ModelCheck;
use crate::session::policy::SharedPolicy;

/// The pack that holds the files of both engines.
const SPEECH_PACK: &str = "asr";

/// Why a load gave no engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadError {
    FileFailed,
    Failed,
}

impl LoadError {
    /// The message of `START_FAILED`. It names no file and no folder.
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::FileFailed => "A model file failed its check.",
            Self::Failed => "The speech models could not be loaded.",
        }
    }
}

type Loaded = Result<Arc<Engines>, LoadError>;

#[derive(Default)]
enum Slot {
    #[default]
    Empty,
    /// One load; later starts wait for its result.
    Loading(Arc<Flight<Loaded>>),
    Ready(Arc<Engines>),
}

/// The engines of this program run.
#[derive(Default)]
pub(super) struct EngineCache(Mutex<Slot>);

impl EngineCache {
    fn slot(&self) -> MutexGuard<'_, Slot> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The loaded engines, or the result of the one load that runs. A start
    /// that finds none loads them on its own thread.
    pub(super) fn load(
        &self,
        models: &ModelCheck,
        machine: &dyn Machine,
        policy: &SharedPolicy,
    ) -> Loaded {
        let flight = {
            let mut slot = self.slot();
            match &*slot {
                // A panic in a call that outlived its session leaves no other
                // trace, so ask the engines.
                Slot::Ready(engines) if !engines.is_broken() => return Ok(Arc::clone(engines)),
                Slot::Loading(flight) => {
                    let flight = Arc::clone(flight);
                    drop(slot);
                    return flight.wait();
                }
                Slot::Ready(_) | Slot::Empty => {
                    let flight = Arc::new(Flight::default());
                    *slot = Slot::Loading(Arc::clone(&flight));
                    flight
                }
            }
        };
        // A load that panics fails like any other; nobody waits for ever.
        let loaded =
            panic::catch_unwind(AssertUnwindSafe(|| load_checked(models, machine, policy)))
                .unwrap_or(Err(LoadError::Failed));
        *self.slot() = match &loaded {
            Ok(engines) => Slot::Ready(Arc::clone(engines)),
            Err(_) => Slot::Empty,
        };
        flight.finish(loaded.clone());
        loaded
    }

    /// Drop the engines, so the next Start loads them again.
    pub(super) fn forget(&self) {
        let mut slot = self.slot();
        if matches!(*slot, Slot::Ready(_)) {
            *slot = Slot::Empty;
        }
    }
}

/// Check the files, load the engines and let the privacy policy admit them.
fn load_checked(models: &ModelCheck, machine: &dyn Machine, policy: &SharedPolicy) -> Loaded {
    if models.check_again(SPEECH_PACK) != PackCheck::Ok {
        return Err(LoadError::FileFailed);
    }
    let folder = models.folder().ok_or(LoadError::FileFailed)?;
    let loaded = machine.engines(folder).map_err(|_| LoadError::Failed)?;
    Engines::new(loaded.vad, loaded.recognizer, Arc::clone(policy))
        .map(Arc::new)
        .map_err(|_| LoadError::Failed)
}
