//! The one session of the app: its refusals, its start, its stop and its end.
//!
//! Slow work (the setup check, the load, opening the microphone) runs without a
//! lock. After each step the start asks whether its session still is the current
//! one. Every event of a session goes out under the phase lock and only while
//! that session is current, so no event of it follows its stop. Lock order:
//! phase, then policy or capture. `LiveChain::stop` waits for a sink call that
//! may hold the phase lock, so it runs without that lock.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use engine_host::live_chain::{Caption, ChainEnd, ChainSink, Clock, Engines, Latency, LiveChain};
use meeting_core::policy::PrivacyPolicy;
use meeting_core::protocol::SessionStatus;

use crate::commands::meeting::{parse_mode, setup_refusal_message};
use crate::commands::setup::SetupAnswer;
use crate::commands::start_checks::{failed_checks, refusal, DeviceFacts};
use crate::events::{BannerEvent, BannerKind, CaptionEvent, LatencyEvent, RunState, SessionSink};
use crate::ipc::{
    Direction, ErrorCode, IpcError, IpcResult, MeetingHandle, MeetingMode, SetMuteArgs,
    SetupStatus, StartMeetingArgs,
};
use crate::session::capture::{mic_config, Capture, Microphone};
use crate::session::engines::EngineCache;
use crate::session::machine::Machine;
use crate::session::model_check::ModelCheck;
use crate::session::policy::{self, SharedPolicy};

const MIC_FAILED: &str = "The microphone could not be opened.";
const SENTENCE_FAILED: &str = "One sentence could not be recognized.";
const SOURCE_LOST: &str = "Microphone disconnected. Your outgoing direction is stopped. \
                           Reconnect it, then press Start.";
const PROCESSING_STOPPED: &str = "Speech processing stopped. Press Start to try again.";

/// Where the session is. The id is the meeting id of the accepted start.
#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    Starting {
        id: String,
    },
    Running {
        id: String,
        chain: LiveChain,
        capture: Box<dyn Capture>,
    },
}

impl Phase {
    fn is_starting(&self, wanted: &str) -> bool {
        matches!(self, Self::Starting { id } if id == wanted)
    }

    fn is_running(&self, wanted: &str) -> bool {
        matches!(self, Self::Running { id, .. } if id == wanted)
    }
}

struct Shared {
    machine: Box<dyn Machine>,
    models: ModelCheck,
    engines: EngineCache,
    policy: SharedPolicy,
    clock: Arc<dyn Clock>,
    phase: Mutex<Phase>,
}

impl Shared {
    /// Every change of the phase is one assignment, so a panic elsewhere cannot
    /// leave it half-written: a poisoned lock is still safe to use.
    fn phase(&self) -> MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One setup check with the checked model rows. Only a running session
    /// keeps the check from opening the microphone.
    fn setup(&self) -> SetupAnswer {
        let running = || matches!(*self.phase(), Phase::Running { .. });
        let mut answer = self.machine.setup(&running);
        self.models.apply(&mut answer.status);
        answer
    }

    fn release(&self, phase: &mut Phase) {
        *phase = Phase::Idle;
        policy::record(&self.policy, SessionStatus::Idle);
    }

    /// The chain of session `id` ended by itself. Runs inside a call of its
    /// sink, so the chain is dropped here and never stopped.
    fn end(&self, id: &str, end: ChainEnd, events: &dyn SessionSink) {
        let mut phase = self.phase();
        if !phase.is_running(id) {
            return;
        }
        if let Phase::Running {
            chain, mut capture, ..
        } = std::mem::take(&mut *phase)
        {
            capture.stop();
            drop(chain);
        }
        if end == ChainEnd::Failed {
            self.engines.forget();
        }
        self.release(&mut phase);
        events.emit_status(RunState::Idle);
        events.emit_banner(BannerEvent {
            kind: BannerKind::Error,
            message: match end {
                ChainEnd::SourceLost => SOURCE_LOST,
                ChainEnd::Failed => PROCESSING_STOPPED,
            }
            .to_string(),
        });
    }
}

/// An accepted start that does not run yet. Dropped while its session is still
/// starting (a panic), it releases the session.
struct Reservation<'a> {
    shared: &'a Arc<Shared>,
    id: String,
    events: Arc<dyn SessionSink>,
    /// `starting` went out, so `idle` has to follow.
    announced: bool,
}

impl Reservation<'_> {
    /// `NO_MEETING` once a stop has cancelled this start.
    fn check(&self) -> IpcResult<()> {
        if self.shared.phase().is_starting(&self.id) {
            Ok(())
        } else {
            Err(IpcError::no_meeting())
        }
    }

    /// Send `starting`.
    fn announce(&mut self) -> IpcResult<()> {
        let phase = self.shared.phase();
        if !phase.is_starting(&self.id) {
            return Err(IpcError::no_meeting());
        }
        self.events.emit_status(RunState::Starting);
        self.announced = true;
        Ok(())
    }

    /// Release the session and give the answer of the start: `error`, or
    /// `NO_MEETING` when a stop came first.
    fn refuse(&self, error: IpcError) -> IpcError {
        let mut phase = self.shared.phase();
        if !phase.is_starting(&self.id) {
            return IpcError::no_meeting();
        }
        self.shared.release(&mut phase);
        if self.announced {
            self.events.emit_status(RunState::Idle);
        }
        error
    }

    /// Start the chain on the open microphone and report `running`.
    fn run(&self, mut microphone: Microphone, engines: Arc<Engines>) -> IpcResult<MeetingHandle> {
        // The first events of the chain wait for this lock, so `running` is
        // sent before them.
        let mut phase = self.shared.phase();
        if !phase.is_starting(&self.id) {
            drop(phase);
            microphone.capture.stop();
            return Err(IpcError::no_meeting());
        }
        let sink = Forward {
            shared: Arc::downgrade(self.shared),
            id: self.id.clone(),
            events: Arc::clone(&self.events),
        };
        let clock = Arc::clone(&self.shared.clock);
        match LiveChain::start(microphone.source, engines, clock, Box::new(sink)) {
            Ok(chain) => {
                *phase = Phase::Running {
                    id: self.id.clone(),
                    chain,
                    capture: microphone.capture,
                };
                policy::record(&self.shared.policy, SessionStatus::Running);
                self.events.emit_status(RunState::Running);
                Ok(MeetingHandle {
                    meeting_id: self.id.clone(),
                })
            }
            Err(_) => {
                drop(phase);
                microphone.capture.stop();
                // The chain only fails when a thread cannot start.
                Err(self.refuse(start_failed(PROCESSING_STOPPED)))
            }
        }
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        // After `refuse`, `run` or a stop, this changes nothing.
        self.refuse(IpcError::no_meeting());
    }
}

/// Passes what the chain of one session produces to the webview.
struct Forward {
    shared: Weak<Shared>,
    id: String,
    events: Arc<dyn SessionSink>,
}

impl Forward {
    /// Run `send` while this session is the running one.
    fn while_current(&self, send: impl FnOnce(&dyn SessionSink)) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let phase = shared.phase();
        if phase.is_running(&self.id) {
            send(self.events.as_ref());
        }
    }
}

impl ChainSink for Forward {
    fn caption(&mut self, caption: Caption) {
        self.while_current(|events| {
            events.emit_caption(CaptionEvent {
                line_id: format!("line-{}", caption.line_id),
                direction: Direction::Out,
                is_final: true,
                source: caption.text,
                target: String::new(),
                delay_ms: wire_ms(caption.delay_ms),
            });
        });
    }

    fn latency(&mut self, latency: Latency) {
        self.while_current(|events| {
            events.emit_latency(LatencyEvent {
                p50_ms: wire_ms(latency.p50_ms),
                p95_ms: wire_ms(latency.p95_ms),
            });
        });
    }

    fn recognition_failed(&mut self) {
        self.while_current(|events| {
            events.emit_banner(BannerEvent {
                kind: BannerKind::Warning,
                message: SENTENCE_FAILED.to_string(),
            });
        });
    }

    fn ended(&mut self, end: ChainEnd) {
        if let Some(shared) = self.shared.upgrade() {
            shared.end(&self.id, end, self.events.as_ref());
        }
    }
}

/// Managed state of the app: the one session and what it reads.
pub struct Session {
    shared: Arc<Shared>,
}

impl Session {
    pub fn new(
        machine: Box<dyn Machine>,
        models: ModelCheck,
        policy: SharedPolicy,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                machine,
                models,
                engines: EngineCache::default(),
                policy,
                clock,
                phase: Mutex::default(),
            }),
        }
    }

    /// True from an accepted Start until the session ends.
    pub fn is_active(&self) -> bool {
        !matches!(*self.shared.phase(), Phase::Idle)
    }

    /// The setup status of this machine, with the model rows of the model check.
    /// Blocks while the launch check or the device probe runs.
    pub fn check_setup(&self) -> SetupStatus {
        self.shared.setup().status
    }

    /// The privacy policy as it is now.
    pub fn privacy_policy(&self) -> PrivacyPolicy {
        policy::current(&self.shared.policy)
    }

    /// Start a session. Blocks until it runs, is refused or fails. Refusal
    /// order: mode, a session that exists, setup, start checks. `starting` is
    /// sent only for a start that passed them all.
    pub fn start(
        &self,
        args: &StartMeetingArgs,
        meeting_id: String,
        events: Arc<dyn SessionSink>,
    ) -> IpcResult<MeetingHandle> {
        if matches!(
            parse_mode(&args.mode)?,
            MeetingMode::Off | MeetingMode::Listen
        ) {
            return Err(IpcError::not_implemented("This mode"));
        }
        let mut start = self.reserve(meeting_id, events)?;

        let mut answer = self.shared.setup();
        if answer.waited {
            // The answer of a check that skipped the microphone is too old.
            answer = self.shared.setup();
        }
        start.check()?;
        if !answer.status.can_start {
            return Err(start.refuse(IpcError::new(
                ErrorCode::SetupIncomplete,
                setup_refusal_message(&answer.status),
            )));
        }
        if let Some(error) = refusal(&failed_checks(args, &DeviceFacts(&answer.devices))) {
            return Err(start.refuse(error));
        }
        start.announce()?;

        let shared = &self.shared;
        let engines = shared
            .engines
            .load(&shared.models, shared.machine.as_ref(), &shared.policy)
            .map_err(|why| start.refuse(start_failed(why.message())))?;
        start.check()?;
        let microphone = mic_config(&answer.devices)
            .and_then(|config| shared.machine.open_microphone(config).ok())
            .ok_or_else(|| start.refuse(start_failed(MIC_FAILED)))?;
        start.run(microphone, engines)
    }

    /// Take the one session for a new start.
    fn reserve(&self, id: String, events: Arc<dyn SessionSink>) -> IpcResult<Reservation<'_>> {
        let mut phase = self.shared.phase();
        if !matches!(*phase, Phase::Idle) {
            return Err(IpcError::meeting_running());
        }
        *phase = Phase::Starting { id: id.clone() };
        policy::record(&self.shared.policy, SessionStatus::Preparing);
        drop(phase);
        Ok(Reservation {
            shared: &self.shared,
            id,
            events,
            announced: false,
        })
    }

    /// End the session. Does not wait for a running engine call or load.
    pub fn stop(&self, events: &dyn SessionSink) -> IpcResult<MeetingHandle> {
        let mut phase = self.shared.phase();
        let (meeting_id, chain) = match std::mem::take(&mut *phase) {
            Phase::Idle => return Err(IpcError::no_meeting()),
            Phase::Starting { id } => (id, None),
            Phase::Running {
                id,
                chain,
                mut capture,
            } => {
                policy::record(&self.shared.policy, SessionStatus::Stopping);
                // First, so that no waiting speech is recognized while the
                // microphone closes.
                chain.cancel();
                capture.stop();
                (id, Some(chain))
            }
        };
        self.shared.release(&mut phase);
        events.emit_status(RunState::Idle);
        drop(phase);
        if let Some(chain) = chain {
            chain.stop();
        }
        Ok(MeetingHandle { meeting_id })
    }

    /// Muting has no audio path yet: a running meeting answers
    /// `NOT_IMPLEMENTED`, never `Ok`.
    pub fn mute(&self, _args: &SetMuteArgs) -> IpcResult<()> {
        if matches!(*self.shared.phase(), Phase::Running { .. }) {
            Err(IpcError::not_implemented("Muting"))
        } else {
            Err(IpcError::no_meeting())
        }
    }
}

/// The answer of a start whose task panicked.
#[cfg(not(feature = "demo-data"))]
pub(crate) fn start_panicked() -> IpcError {
    start_failed(PROCESSING_STOPPED)
}

fn start_failed(message: &str) -> IpcError {
    IpcError::new(ErrorCode::StartFailed, message)
}

/// Milliseconds as the events carry them.
fn wire_ms(milliseconds: u64) -> u32 {
    u32::try_from(milliseconds).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::testing::Rig;
    use std::thread;

    #[test]
    fn an_event_goes_out_only_while_its_session_runs() {
        let rig = Rig::new();
        let warn = |id: &str| {
            let mut sink = Forward {
                shared: Arc::downgrade(&rig.session.shared),
                id: id.to_string(),
                events: Arc::new(rig.events.clone()),
            };
            sink.recognition_failed();
        };
        let warnings = || rig.events.banners().len();

        warn("m1");
        assert_eq!(warnings(), 0, "without a session");
        rig.fake.load_gate.hold();
        thread::scope(|scope| {
            let held = scope.spawn(|| rig.start());
            rig.fake.load_gate.wait_for_arrival(1);
            warn("m1");
            assert_eq!(warnings(), 0, "while its session is starting");
            rig.fake.load_gate.release();
            held.join().unwrap().unwrap();
        });
        warn("another");
        assert_eq!(warnings(), 0, "of another session");
        warn("m1");
        assert_eq!(warnings(), 1, "of the running session");
    }
}
