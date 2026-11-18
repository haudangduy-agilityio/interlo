//! quick-actions: translate, fix grammar and save a note from any app.
//!
//! Owns: the action list, the guard that decides whether a selection may be
//! read, text chunking for long selections, and the preview state machine
//! (Idle, Preview, Pasted or Cancelled).
//!
//! Must never: read a password field, log keystrokes, or paste without the
//! owner's confirm. `guard` refuses secure input; the state machine reaches
//! `Pasted` only on Enter, `ENTER_GUARD_MS` after a result is shown, and never
//! records what is typed.

use thiserror::Error;

/// Default size of one chunk sent to a translator or grammar fixer.
pub const DEFAULT_MAX_WORDS: usize = 1000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuickError {
    #[error("secure input is active: the selection is not read")]
    SecureInput,
    #[error("app {0} is on the exclusion list")]
    ExcludedApp(String),
    #[error("the app is unknown, so it cannot be checked against the exclusion list")]
    UnknownApp,
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAction {
    Translate,
    FixGrammar,
    SaveNote,
}

/// What the host app knows about the place the selection comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionContext {
    /// Bundle id or process name of the front app.
    pub app_id: String,
    /// True while a password field or other secure input has focus.
    pub secure_input: bool,
}

/// Decide whether the selection may be read. Secure input first, then the
/// exclusion list (case-insensitive). An empty app id is refused.
pub fn guard(ctx: &SelectionContext, exclusion_list: &[String]) -> Result<(), QuickError> {
    if ctx.secure_input {
        return Err(QuickError::SecureInput);
    }
    let app = ctx.app_id.trim();
    if app.is_empty() {
        return Err(QuickError::UnknownApp);
    }
    if exclusion_list
        .iter()
        .any(|e| e.trim().eq_ignore_ascii_case(app))
    {
        return Err(QuickError::ExcludedApp(app.to_string()));
    }
    Ok(())
}

/// Cut `text` into chunks of at most `max_words` words (at least 1). Chunks
/// break at word starts and keep every character, so joining them gives back
/// the original text. Text with no words gives no chunks.
pub fn split_words(text: &str, max_words: usize) -> Vec<String> {
    let max = max_words.max(1);
    let mut starts: Vec<usize> = Vec::new();
    let mut in_word = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            starts.push(i);
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }
    let mut cuts: Vec<usize> = starts.iter().copied().step_by(max).collect();
    cuts[0] = 0;
    cuts.push(text.len());
    cuts.windows(2)
        .map(|w| text[w[0]..w[1]].to_string())
        .collect()
}

/// Enter is ignored for this long after a result is shown, so a key press
/// that was already on its way cannot confirm a result the owner never saw.
/// Starting value.
pub const ENTER_GUARD_MS: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    Idle,
    Preview {
        action: QuickAction,
        text: String,
        /// When the result was shown, on the caller's clock.
        shown_at_ms: u64,
    },
    /// The owner confirmed with Enter. The text may now be pasted or saved.
    Pasted {
        action: QuickAction,
        text: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewEvent {
    /// A result is ready to show.
    Show { action: QuickAction, text: String },
    /// The owner pressed Enter.
    Enter,
    /// The owner pressed Esc.
    Esc,
    /// The owner typed something. The key itself is never passed in.
    Typed,
    /// Back to Idle after a finished preview.
    Reset,
}

/// One step of the preview state machine. `now_ms` is the caller's clock.
/// - Idle + Show -> Preview
/// - Preview + Show -> Preview with the newer result
/// - Preview + Enter -> Pasted, only `ENTER_GUARD_MS` after the result was shown
/// - Preview + Esc -> Cancelled
/// - Typed never changes the state.
/// - Pasted and Cancelled only leave through Reset.
pub fn step(state: PreviewState, event: PreviewEvent, now_ms: u64) -> PreviewState {
    match (state, event) {
        (PreviewState::Idle, PreviewEvent::Show { action, text }) => PreviewState::Preview {
            action,
            text,
            shown_at_ms: now_ms,
        },
        (
            PreviewState::Preview {
                action,
                text,
                shown_at_ms,
            },
            PreviewEvent::Enter,
        ) if now_ms.saturating_sub(shown_at_ms) >= ENTER_GUARD_MS => {
            PreviewState::Pasted { action, text }
        }
        (PreviewState::Preview { .. }, PreviewEvent::Show { action, text }) => {
            PreviewState::Preview {
                action,
                text,
                shown_at_ms: now_ms,
            }
        }
        (PreviewState::Preview { .. }, PreviewEvent::Esc) => PreviewState::Cancelled,
        (PreviewState::Pasted { .. } | PreviewState::Cancelled, PreviewEvent::Reset) => {
            PreviewState::Idle
        }
        (state, _) => state,
    }
}

/// Run one action on one chunk of text. Stub: providers come in a later task.
pub fn run_action(_action: QuickAction, _chunk: &str) -> Result<String, QuickError> {
    Err(QuickError::NotImplemented("run_action"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(app: &str, secure: bool) -> SelectionContext {
        SelectionContext {
            app_id: app.into(),
            secure_input: secure,
        }
    }

    fn excluded() -> Vec<String> {
        vec!["com.example.vault".into(), "Com.Example.Bank".into()]
    }

    fn words(n: usize) -> String {
        (0..n)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn count_words(s: &str) -> usize {
        s.split_whitespace().count()
    }

    #[test]
    fn normal_app_passes_the_guard() {
        assert_eq!(
            guard(&ctx("com.example.editor", false), &excluded()),
            Ok(())
        );
    }

    #[test]
    fn secure_input_is_refused_first() {
        assert_eq!(
            guard(&ctx("com.example.editor", true), &excluded()),
            Err(QuickError::SecureInput)
        );
        // Even for an excluded app, secure input is the reported reason.
        assert_eq!(
            guard(&ctx("com.example.vault", true), &excluded()),
            Err(QuickError::SecureInput)
        );
    }

    #[test]
    fn excluded_app_is_refused_without_case_rules() {
        assert_eq!(
            guard(&ctx("com.example.vault", false), &excluded()),
            Err(QuickError::ExcludedApp("com.example.vault".into()))
        );
        assert!(matches!(
            guard(&ctx("COM.EXAMPLE.BANK", false), &excluded()),
            Err(QuickError::ExcludedApp(_))
        ));
    }

    #[test]
    fn unknown_app_fails_closed() {
        assert_eq!(guard(&ctx("  ", false), &[]), Err(QuickError::UnknownApp));
    }

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(
            split_words("hello there", DEFAULT_MAX_WORDS),
            vec!["hello there"]
        );
    }

    #[test]
    fn long_text_is_cut_at_the_word_limit() {
        let text = words(2_500);
        let chunks = split_words(&text, DEFAULT_MAX_WORDS);
        let sizes: Vec<usize> = chunks.iter().map(|c| count_words(c)).collect();
        assert_eq!(sizes, vec![1000, 1000, 500]);
        assert_eq!(chunks.concat(), text, "no character is lost");
    }

    #[test]
    fn exact_multiple_has_no_empty_chunk() {
        let chunks = split_words(&words(2_000), DEFAULT_MAX_WORDS);
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| count_words(c) == 1000));
    }

    #[test]
    fn whitespace_and_newlines_are_kept() {
        let text = "  one two\n\nthree   four  ";
        let chunks = split_words(text, 3);
        assert_eq!(chunks, vec!["  one two\n\nthree   ", "four  "]);
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn no_words_gives_no_chunks() {
        assert!(split_words("", 10).is_empty());
        assert!(split_words(" \n\t ", 10).is_empty());
    }

    #[test]
    fn zero_limit_is_treated_as_one_word() {
        assert_eq!(split_words("a b c", 0), vec!["a ", "b ", "c"]);
    }

    #[test]
    fn non_ascii_text_splits_on_char_boundaries() {
        let text = "xin chào các bạn";
        let chunks = split_words(text, 2);
        assert_eq!(chunks, vec!["xin chào ", "các bạn"]);
    }

    const T0: u64 = 10_000;

    fn show() -> PreviewEvent {
        PreviewEvent::Show {
            action: QuickAction::FixGrammar,
            text: "Fixed text.".into(),
        }
    }

    /// A preview shown at `T0`.
    fn preview() -> PreviewState {
        step(PreviewState::Idle, show(), T0)
    }

    fn pasted() -> PreviewState {
        PreviewState::Pasted {
            action: QuickAction::FixGrammar,
            text: "Fixed text.".into(),
        }
    }

    #[test]
    fn idle_shows_a_preview() {
        assert_eq!(
            preview(),
            PreviewState::Preview {
                action: QuickAction::FixGrammar,
                text: "Fixed text.".into(),
                shown_at_ms: T0
            }
        );
    }

    #[test]
    fn enter_pastes_and_esc_cancels() {
        assert_eq!(
            step(preview(), PreviewEvent::Enter, T0 + ENTER_GUARD_MS),
            pasted()
        );
        assert_eq!(
            step(preview(), PreviewEvent::Esc, T0),
            PreviewState::Cancelled
        );
    }

    #[test]
    fn enter_is_ignored_for_300_ms_after_show() {
        assert_eq!(ENTER_GUARD_MS, 300);
        assert_eq!(
            step(preview(), PreviewEvent::Enter, T0 + 299),
            preview(),
            "299 ms is too early"
        );
        assert_eq!(step(preview(), PreviewEvent::Enter, T0), preview());
        assert_eq!(step(preview(), PreviewEvent::Enter, T0 + 300), pasted());
    }

    #[test]
    fn a_clock_that_runs_backwards_does_not_confirm() {
        assert_eq!(step(preview(), PreviewEvent::Enter, T0 - 5_000), preview());
    }

    #[test]
    fn esc_works_inside_the_enter_guard() {
        assert_eq!(
            step(preview(), PreviewEvent::Esc, T0 + 1),
            PreviewState::Cancelled
        );
    }

    #[test]
    fn a_new_show_replaces_the_preview_with_the_newer_result() {
        let other = PreviewEvent::Show {
            action: QuickAction::Translate,
            text: "Other".into(),
        };
        let replaced = step(preview(), other, T0 + 5_000);
        assert_eq!(
            replaced,
            PreviewState::Preview {
                action: QuickAction::Translate,
                text: "Other".into(),
                shown_at_ms: T0 + 5_000
            }
        );
    }

    #[test]
    fn enter_guard_restarts_when_a_newer_result_replaces_the_preview() {
        let other = PreviewEvent::Show {
            action: QuickAction::Translate,
            text: "Other".into(),
        };
        let replaced = step(preview(), other, T0 + 5_000);
        // 299 ms after the new result: ignored, even though the first one is old.
        assert_eq!(
            step(replaced.clone(), PreviewEvent::Enter, T0 + 5_299),
            replaced
        );
        assert!(matches!(
            step(replaced, PreviewEvent::Enter, T0 + 5_300),
            PreviewState::Pasted {
                action: QuickAction::Translate,
                ..
            }
        ));
    }

    #[test]
    fn typing_never_changes_the_state() {
        let mut s = preview();
        for i in 0..5 {
            s = step(s, PreviewEvent::Typed, T0 + 1_000 * i);
        }
        assert_eq!(s, preview());
        for state in [PreviewState::Idle, pasted(), PreviewState::Cancelled] {
            assert_eq!(step(state.clone(), PreviewEvent::Typed, T0), state);
        }
    }

    #[test]
    fn nothing_is_pasted_without_a_preview_and_enter() {
        // Enter or Esc while idle do nothing.
        assert_eq!(
            step(PreviewState::Idle, PreviewEvent::Enter, T0),
            PreviewState::Idle
        );
        assert_eq!(
            step(PreviewState::Idle, PreviewEvent::Esc, T0),
            PreviewState::Idle
        );
    }

    #[test]
    fn finished_previews_only_leave_through_reset() {
        let done = step(preview(), PreviewEvent::Enter, T0 + 300);
        assert_eq!(step(done.clone(), PreviewEvent::Esc, T0 + 400), done);
        assert_eq!(step(done.clone(), show(), T0 + 400), done);
        assert_eq!(
            step(done, PreviewEvent::Reset, T0 + 500),
            PreviewState::Idle
        );

        let cancelled = step(preview(), PreviewEvent::Esc, T0 + 1);
        assert_eq!(
            step(cancelled.clone(), PreviewEvent::Enter, T0 + 1_000),
            cancelled
        );
        assert_eq!(
            step(cancelled, PreviewEvent::Reset, T0 + 1_000),
            PreviewState::Idle
        );

        // Reset does nothing to an open preview.
        assert_eq!(step(preview(), PreviewEvent::Reset, T0 + 1_000), preview());
    }

    #[test]
    fn run_action_is_a_stub() {
        assert!(matches!(
            run_action(QuickAction::Translate, "hi"),
            Err(QuickError::NotImplemented(_))
        ));
    }
}
