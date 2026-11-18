//! summary-jobs: job queue rules, summarizer calls, citation check and question answerer.
//!
//! Owns: the retry and lease policy of summary jobs (storage-agnostic), the
//! `Summarizer` and `QuestionAnswerer` traits, the citation check on every
//! point, and the rule that decides whether a new reply may replace the notes.
//!
//! Must never: replace good notes with a bad reply, or keep a point without
//! evidence line ids. Only points whose ids all exist in the transcript pass
//! `check_citations`, and `decide_notes` keeps the old notes when the reply has
//! a rejected point.

use meeting_core::policy::{EgressDenied, PrivacyPolicy, Provider};
use meeting_core::protocol::DataKind;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// A job runs at most this many times.
pub const MAX_ATTEMPTS: u32 = 3;
/// Wait before the 2nd and the 3rd attempt: 30 s, then 60 s.
pub const RETRY_DELAYS_MS: [u64; 2] = [30_000, 60_000];
/// A claimed job must finish within 3 minutes, or its lease is lost.
pub const LEASE_MS: u64 = 180_000;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("job is not due to run")]
    NotDue,
    #[error("job is {0:?}, not leased")]
    NotLeased(JobState),
    #[error(transparent)]
    Egress(#[from] EgressDenied),
    #[error("question answerer {0} is remote; answers stay local")]
    RemoteAnswerer(String),
    #[error("answer has no evidence line ids")]
    AnswerWithoutEvidence,
    #[error("answer cites unknown line id {0}")]
    AnswerUnknownLine(String),
    #[error("backend failed: {0}")]
    Backend(String),
}

/// When to retry after attempt number `attempt` (1-based) failed at `now_ms`.
/// Returns `None` when no attempt is left (or `attempt` is 0, which never ran).
pub fn next_retry_at(attempt: u32, now_ms: u64) -> Option<u64> {
    if attempt == 0 || attempt >= MAX_ATTEMPTS {
        return None;
    }
    RETRY_DELAYS_MS
        .get((attempt - 1) as usize)
        .map(|delay| now_ms.saturating_add(*delay))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Summarize,
    AnswerQuestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Leased,
    Done,
    /// Out of attempts. Needs the owner's action.
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub meeting_id: String,
    pub kind: JobKind,
    pub state: JobState,
    /// Attempts started so far.
    pub attempts: u32,
    pub run_at_ms: u64,
    pub lease_until_ms: Option<u64>,
}

impl Job {
    pub fn new(id: &str, meeting_id: &str, kind: JobKind, now_ms: u64) -> Self {
        Self {
            id: id.to_string(),
            meeting_id: meeting_id.to_string(),
            kind,
            state: JobState::Queued,
            attempts: 0,
            run_at_ms: now_ms,
            lease_until_ms: None,
        }
    }

    pub fn is_due(&self, now_ms: u64) -> bool {
        self.state == JobState::Queued && self.run_at_ms <= now_ms
    }

    pub fn lease_expired(&self, now_ms: u64) -> bool {
        self.state == JobState::Leased && self.lease_until_ms.is_some_and(|t| t <= now_ms)
    }

    /// Start an attempt and take a lease of `LEASE_MS`.
    pub fn claim(&mut self, now_ms: u64) -> Result<(), JobError> {
        if !self.is_due(now_ms) {
            return Err(JobError::NotDue);
        }
        self.attempts += 1;
        self.state = JobState::Leased;
        self.lease_until_ms = Some(now_ms.saturating_add(LEASE_MS));
        Ok(())
    }

    /// The running attempt succeeded. Only a leased job can complete: a late
    /// worker whose lease was reaped (the job is `Queued` again, or `Dead`)
    /// gets `NotLeased` and the job is left as it is.
    pub fn complete(&mut self) -> Result<(), JobError> {
        self.require_leased()?;
        self.state = JobState::Done;
        self.lease_until_ms = None;
        Ok(())
    }

    /// The running attempt failed: queue a retry, or give up. Only a leased
    /// job can fail; any other state returns `NotLeased` and changes nothing,
    /// so a fresh queued job never goes `Dead` from a stray call.
    pub fn fail(&mut self, now_ms: u64) -> Result<(), JobError> {
        self.require_leased()?;
        self.lease_until_ms = None;
        match next_retry_at(self.attempts, now_ms) {
            Some(at) => {
                self.state = JobState::Queued;
                self.run_at_ms = at;
            }
            None => self.state = JobState::Dead,
        }
        Ok(())
    }

    /// A worker that died leaves a lease behind. Treat an expired lease as a
    /// failed attempt. Returns true when it did.
    pub fn reap_expired_lease(&mut self, now_ms: u64) -> bool {
        self.lease_expired(now_ms) && self.fail(now_ms).is_ok()
    }

    fn require_leased(&self) -> Result<(), JobError> {
        if self.state == JobState::Leased {
            Ok(())
        } else {
            Err(JobError::NotLeased(self.state))
        }
    }
}

/// One line of the transcript that a summarizer may cite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptLine {
    pub line_id: String,
    pub text: String,
}

/// One bullet of the notes, with the transcript lines that prove it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub text: String,
    pub line_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    NoEvidence,
    UnknownLineIds(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedPoint {
    pub point: Point,
    pub reason: RejectReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CitationReport {
    pub accepted: Vec<Point>,
    pub rejected: Vec<RejectedPoint>,
}

impl CitationReport {
    /// At least one point and none rejected.
    pub fn is_clean(&self) -> bool {
        !self.accepted.is_empty() && self.rejected.is_empty()
    }
}

/// True when there is at least one id and none of them is blank. A blank id
/// ("" or only whitespace) is never evidence, even next to a real one.
fn has_only_real_ids(line_ids: &[String]) -> bool {
    !line_ids.is_empty() && line_ids.iter().all(|i| !i.trim().is_empty())
}

/// Keep points whose line ids are all in the transcript. A point with no ids,
/// any blank id, or any unknown id is rejected.
pub fn check_citations(points: &[Point], known_line_ids: &[String]) -> CitationReport {
    let known: HashSet<&str> = known_line_ids.iter().map(String::as_str).collect();
    let mut report = CitationReport::default();
    for p in points {
        if !has_only_real_ids(&p.line_ids) {
            report.rejected.push(RejectedPoint {
                point: p.clone(),
                reason: RejectReason::NoEvidence,
            });
            continue;
        }
        let unknown: Vec<String> = p
            .line_ids
            .iter()
            .filter(|i| !known.contains(i.as_str()))
            .cloned()
            .collect();
        if unknown.is_empty() {
            report.accepted.push(p.clone());
        } else {
            report.rejected.push(RejectedPoint {
                point: p.clone(),
                reason: RejectReason::UnknownLineIds(unknown),
            });
        }
    }
    report
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesDecision {
    Keep,
    Replace(Vec<Point>),
}

/// Decide what to store after a summary run.
/// - No accepted point: keep what exists.
/// - Notes exist and the reply has any rejected point: keep what exists.
/// - Otherwise store the accepted points (first notes may drop bad points).
pub fn decide_notes(existing: &[Point], report: &CitationReport) -> NotesDecision {
    if report.accepted.is_empty() {
        return NotesDecision::Keep;
    }
    if !existing.is_empty() && !report.rejected.is_empty() {
        return NotesDecision::Keep;
    }
    NotesDecision::Replace(report.accepted.clone())
}

pub struct SummaryRequest {
    pub meeting_id: String,
    pub lines: Vec<TranscriptLine>,
}

pub trait Summarizer {
    fn provider(&self) -> Provider;
    fn summarize(&self, req: &SummaryRequest) -> Result<Vec<Point>, JobError>;
}

/// Summarizer with no backend yet. Local, so it passes the egress guard.
pub struct UnavailableSummarizer;

impl Summarizer for UnavailableSummarizer {
    fn provider(&self) -> Provider {
        Provider::local("unavailable")
    }

    fn summarize(&self, _req: &SummaryRequest) -> Result<Vec<Point>, JobError> {
        Err(JobError::NotImplemented("summarize"))
    }
}

/// Run one summary: egress check, call, citation check, notes decision.
pub fn run_summary(
    summarizer: &dyn Summarizer,
    policy: &PrivacyPolicy,
    req: &SummaryRequest,
    existing: &[Point],
) -> Result<NotesDecision, JobError> {
    policy.check(&summarizer.provider(), DataKind::Text)?;
    let points = summarizer.summarize(req)?;
    let known: Vec<String> = req.lines.iter().map(|l| l.line_id.clone()).collect();
    Ok(decide_notes(existing, &check_citations(&points, &known)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub text: String,
    pub line_ids: Vec<String>,
}

/// Answers a question about one meeting. Local only: `ask_local` refuses a
/// remote implementation. Every answer cites transcript line ids.
pub trait QuestionAnswerer {
    fn provider(&self) -> Provider;
    fn answer(&self, question: &str, lines: &[TranscriptLine]) -> Result<Answer, JobError>;
}

/// Ask a local answerer and check that the answer cites known lines.
pub fn ask_local(
    answerer: &dyn QuestionAnswerer,
    question: &str,
    lines: &[TranscriptLine],
) -> Result<Answer, JobError> {
    let provider = answerer.provider();
    if provider.is_remote() {
        return Err(JobError::RemoteAnswerer(provider.id().to_string()));
    }
    let answer = answerer.answer(question, lines)?;
    if !has_only_real_ids(&answer.line_ids) {
        return Err(JobError::AnswerWithoutEvidence);
    }
    for id in &answer.line_ids {
        if !lines.iter().any(|l| l.line_id.as_str() == id.as_str()) {
            return Err(JobError::AnswerUnknownLine(id.clone()));
        }
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn point(text: &str, line_ids: &[&str]) -> Point {
        Point {
            text: text.into(),
            line_ids: ids(line_ids),
        }
    }

    fn lines() -> Vec<TranscriptLine> {
        ["l1", "l2", "l3"]
            .iter()
            .map(|id| TranscriptLine {
                line_id: id.to_string(),
                text: format!("text {id}"),
            })
            .collect()
    }

    #[test]
    fn policy_constants_match_the_decision() {
        assert_eq!(MAX_ATTEMPTS, 3);
        assert_eq!(RETRY_DELAYS_MS, [30_000, 60_000]);
        assert_eq!(LEASE_MS, 180_000);
    }

    #[test]
    fn retry_schedule_is_30s_then_60s_then_stop() {
        assert_eq!(next_retry_at(1, 1_000), Some(31_000));
        assert_eq!(next_retry_at(2, 1_000), Some(61_000));
        assert_eq!(next_retry_at(3, 1_000), None);
        assert_eq!(next_retry_at(9, 1_000), None);
        assert_eq!(next_retry_at(0, 1_000), None);
        assert_eq!(next_retry_at(1, u64::MAX), Some(u64::MAX));
    }

    #[test]
    fn job_runs_three_times_then_dies() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        assert!(j.is_due(0));

        j.claim(0).unwrap();
        assert_eq!(j.lease_until_ms, Some(LEASE_MS));
        j.fail(10).unwrap();
        assert_eq!((j.state, j.run_at_ms), (JobState::Queued, 30_010));
        assert!(j.claim(30_009).is_err());

        j.claim(30_010).unwrap();
        j.fail(40_000).unwrap();
        assert_eq!((j.state, j.run_at_ms), (JobState::Queued, 100_000));

        j.claim(100_000).unwrap();
        j.fail(100_500).unwrap();
        assert_eq!(j.state, JobState::Dead);
        assert_eq!(j.attempts, 3);
        assert!(j.claim(1_000_000).is_err());
    }

    #[test]
    fn completed_job_is_not_due() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        j.claim(0).unwrap();
        j.complete().unwrap();
        assert_eq!(j.state, JobState::Done);
        assert!(!j.is_due(u64::MAX));
    }

    #[test]
    fn expired_lease_counts_as_a_failed_attempt() {
        let mut j = Job::new("j1", "m1", JobKind::AnswerQuestion, 0);
        j.claim(1_000).unwrap();
        assert!(!j.reap_expired_lease(1_000 + LEASE_MS - 1));
        assert_eq!(j.state, JobState::Leased);

        assert!(j.reap_expired_lease(1_000 + LEASE_MS));
        assert_eq!(j.state, JobState::Queued);
        assert_eq!(j.run_at_ms, 1_000 + LEASE_MS + 30_000);
    }

    #[test]
    fn late_complete_after_the_lease_was_reaped_is_refused() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        j.claim(1_000).unwrap();
        assert!(j.reap_expired_lease(1_000 + LEASE_MS));
        let before = j.clone();

        let err = j.complete().unwrap_err();
        assert!(matches!(err, JobError::NotLeased(JobState::Queued)));
        assert_eq!(j, before, "a refused complete changes nothing");
        assert_ne!(j.state, JobState::Done);
    }

    #[test]
    fn late_complete_on_a_dead_job_is_refused() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        let mut now = 0;
        for _ in 0..MAX_ATTEMPTS {
            j.claim(now).unwrap();
            j.fail(now).unwrap();
            now += 1_000_000;
        }
        assert_eq!(j.state, JobState::Dead);
        assert!(matches!(
            j.complete(),
            Err(JobError::NotLeased(JobState::Dead))
        ));
        assert_eq!(j.state, JobState::Dead);
    }

    #[test]
    fn complete_needs_a_lease_and_a_done_job_cannot_complete_twice() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        assert!(matches!(
            j.complete(),
            Err(JobError::NotLeased(JobState::Queued))
        ));
        j.claim(0).unwrap();
        j.complete().unwrap();
        assert!(matches!(
            j.complete(),
            Err(JobError::NotLeased(JobState::Done))
        ));
    }

    #[test]
    fn fail_on_a_fresh_queued_job_does_not_kill_it() {
        let mut j = Job::new("j1", "m1", JobKind::Summarize, 0);
        let before = j.clone();
        let err = j.fail(5).unwrap_err();
        assert!(matches!(err, JobError::NotLeased(JobState::Queued)));
        assert_eq!(j, before);
        assert_eq!(j.state, JobState::Queued);
        assert!(j.is_due(5), "the job is still due");
    }

    #[test]
    fn fail_on_a_done_or_dead_job_changes_nothing() {
        let mut done = Job::new("j1", "m1", JobKind::Summarize, 0);
        done.claim(0).unwrap();
        done.complete().unwrap();
        let before = done.clone();
        assert!(matches!(
            done.fail(5),
            Err(JobError::NotLeased(JobState::Done))
        ));
        assert_eq!(done, before);

        let mut dead = Job::new("j2", "m1", JobKind::Summarize, 0);
        let mut now = 0;
        for _ in 0..MAX_ATTEMPTS {
            dead.claim(now).unwrap();
            dead.fail(now).unwrap();
            now += 1_000_000;
        }
        let before = dead.clone();
        assert!(matches!(
            dead.fail(now),
            Err(JobError::NotLeased(JobState::Dead))
        ));
        assert_eq!(dead, before);
    }

    #[test]
    fn citation_check_rejects_missing_and_unknown_ids() {
        let known = ids(&["l1", "l2"]);
        let pts = vec![
            point("good", &["l1", "l2"]),
            point("no ids", &[]),
            point("blank id", &["  "]),
            point("unknown", &["l1", "l9"]),
        ];
        let r = check_citations(&pts, &known);
        assert_eq!(r.accepted, vec![point("good", &["l1", "l2"])]);
        assert_eq!(r.rejected.len(), 3);
        assert_eq!(r.rejected[0].reason, RejectReason::NoEvidence);
        assert_eq!(r.rejected[1].reason, RejectReason::NoEvidence);
        assert_eq!(
            r.rejected[2].reason,
            RejectReason::UnknownLineIds(ids(&["l9"]))
        );
        assert!(!r.is_clean());
    }

    #[test]
    fn a_blank_id_next_to_a_real_id_is_not_evidence() {
        let known = ids(&["l1", "l2"]);
        let pts = vec![
            point("probe", &["l1", "  "]),
            point("empty string", &["l1", ""]),
            point("tab", &["\t", "l2"]),
            point("all blank", &["", " "]),
            point("good", &["l1"]),
        ];
        let r = check_citations(&pts, &known);
        assert_eq!(r.accepted, vec![point("good", &["l1"])]);
        assert_eq!(r.rejected.len(), 4);
        for rej in &r.rejected {
            assert_eq!(rej.reason, RejectReason::NoEvidence, "{:?}", rej.point);
        }
        // A reply that holds such a point never replaces existing notes.
        let old = vec![point("old", &["l1"])];
        assert_eq!(decide_notes(&old, &r), NotesDecision::Keep);
    }

    #[test]
    fn an_answer_with_a_blank_id_is_refused() {
        let l = lines();
        assert!(matches!(
            ask_local(&fake(false, vec![], &["l1", "  "]), "q?", &l),
            Err(JobError::AnswerWithoutEvidence)
        ));
        assert!(matches!(
            ask_local(&fake(false, vec![], &["", "l1"]), "q?", &l),
            Err(JobError::AnswerWithoutEvidence)
        ));
        assert!(matches!(
            ask_local(&fake(false, vec![], &["  "]), "q?", &l),
            Err(JobError::AnswerWithoutEvidence)
        ));
    }

    #[test]
    fn notes_are_kept_when_the_reply_is_bad() {
        let old = vec![point("old", &["l1"])];
        let known = ids(&["l1", "l2"]);

        // Every point rejected: keep.
        let bad = check_citations(&[point("x", &[])], &known);
        assert_eq!(decide_notes(&old, &bad), NotesDecision::Keep);

        // Mixed reply while notes exist: keep.
        let mixed = check_citations(&[point("a", &["l2"]), point("b", &["zz"])], &known);
        assert_eq!(decide_notes(&old, &mixed), NotesDecision::Keep);

        // Empty reply: keep.
        assert_eq!(
            decide_notes(&old, &check_citations(&[], &known)),
            NotesDecision::Keep
        );

        // Clean reply: replace.
        let good = check_citations(&[point("a", &["l2"])], &known);
        assert_eq!(
            decide_notes(&old, &good),
            NotesDecision::Replace(vec![point("a", &["l2"])])
        );

        // First notes may drop their bad points.
        assert_eq!(
            decide_notes(&[], &mixed),
            NotesDecision::Replace(vec![point("a", &["l2"])])
        );
    }

    struct Fake {
        remote: bool,
        points: Vec<Point>,
        answer: Answer,
    }

    impl Summarizer for Fake {
        fn provider(&self) -> Provider {
            if self.remote {
                Provider::remote("fake")
            } else {
                Provider::local("fake")
            }
        }
        fn summarize(&self, _req: &SummaryRequest) -> Result<Vec<Point>, JobError> {
            Ok(self.points.clone())
        }
    }

    impl QuestionAnswerer for Fake {
        fn provider(&self) -> Provider {
            if self.remote {
                Provider::remote("fake")
            } else {
                Provider::local("fake")
            }
        }
        fn answer(&self, _q: &str, _l: &[TranscriptLine]) -> Result<Answer, JobError> {
            Ok(self.answer.clone())
        }
    }

    fn fake(remote: bool, points: Vec<Point>, line_ids: &[&str]) -> Fake {
        Fake {
            remote,
            points,
            answer: Answer {
                text: "answer".into(),
                line_ids: ids(line_ids),
            },
        }
    }

    fn request() -> SummaryRequest {
        SummaryRequest {
            meeting_id: "m1".into(),
            lines: lines(),
        }
    }

    #[test]
    fn run_summary_replaces_only_with_cited_points() {
        let policy = PrivacyPolicy::local_only();
        let old = vec![point("old", &["l1"])];

        let good = fake(false, vec![point("new", &["l3"])], &[]);
        assert_eq!(
            run_summary(&good, &policy, &request(), &old).unwrap(),
            NotesDecision::Replace(vec![point("new", &["l3"])])
        );

        let bad = fake(false, vec![point("new", &["nope"])], &[]);
        assert_eq!(
            run_summary(&bad, &policy, &request(), &old).unwrap(),
            NotesDecision::Keep
        );
    }

    #[test]
    fn run_summary_obeys_the_egress_guard() {
        let remote = fake(true, vec![point("new", &["l1"])], &[]);
        let err = run_summary(&remote, &PrivacyPolicy::local_only(), &request(), &[]).unwrap_err();
        assert!(matches!(err, JobError::Egress(_)));

        let mut policy = PrivacyPolicy::local_only();
        policy.set_local_only(false);
        policy.grant("fake", DataKind::Text);
        assert!(run_summary(&remote, &policy, &request(), &[]).is_ok());
    }

    #[test]
    fn unavailable_summarizer_is_a_stub() {
        let err = run_summary(
            &UnavailableSummarizer,
            &PrivacyPolicy::local_only(),
            &request(),
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, JobError::NotImplemented(_)));
    }

    #[test]
    fn answers_must_cite_known_lines() {
        let l = lines();
        assert!(ask_local(&fake(false, vec![], &["l2"]), "q?", &l).is_ok());

        assert!(matches!(
            ask_local(&fake(false, vec![], &[]), "q?", &l),
            Err(JobError::AnswerWithoutEvidence)
        ));
        assert!(matches!(
            ask_local(&fake(false, vec![], &["l2", "l8"]), "q?", &l),
            Err(JobError::AnswerUnknownLine(id)) if id == "l8"
        ));
    }

    #[test]
    fn remote_answerer_is_refused() {
        let l = lines();
        assert!(matches!(
            ask_local(&fake(true, vec![], &["l1"]), "q?", &l),
            Err(JobError::RemoteAnswerer(_))
        ));
    }
}
