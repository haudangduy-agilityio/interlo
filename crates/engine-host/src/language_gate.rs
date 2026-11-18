//! Language gate for the owner's own speech.

use meeting_core::protocol::Language;

/// The owner is expected to speak `expected`. Speech detected in any other
/// language is not translated (for example the owner speaking English in a
/// Vietnamese session); the caption notes that it was skipped.
pub fn owner_should_translate(detected: Language, expected: Language) -> bool {
    detected == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_language_is_translated() {
        assert!(owner_should_translate(Language::Vi, Language::Vi));
    }

    #[test]
    fn owner_speaking_english_is_not_translated() {
        assert!(!owner_should_translate(Language::En, Language::Vi));
    }

    #[test]
    fn rule_translates_exactly_when_detected_matches_expected() {
        let all = [Language::En, Language::Vi];
        for detected in all {
            for expected in all {
                assert_eq!(
                    owner_should_translate(detected, expected),
                    detected == expected,
                    "detected {detected:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn rule_gives_the_same_answer_with_the_pair_swapped() {
        let all = [Language::En, Language::Vi];
        for a in all {
            for b in all {
                assert_eq!(owner_should_translate(a, b), owner_should_translate(b, a));
            }
        }
    }

    #[test]
    fn both_mismatch_directions_are_skipped() {
        assert!(!owner_should_translate(Language::En, Language::Vi));
        assert!(!owner_should_translate(Language::Vi, Language::En));
    }
}
