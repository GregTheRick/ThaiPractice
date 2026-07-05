// Leitner spaced repetition. Boxes 1-5; correct answers climb, wrong answers
// fall back to box 1 with interval 0 so the word re-enters the current session.
pub const MODES: [&str; 4] = ["spell", "read", "translate", "listen"];

const INTERVAL_DAYS: [i64; 5] = [0, 1, 3, 7, 21];
const MAX_BOX: i64 = 5;

/// Returns (new_box, seconds until the word is due again).
pub fn leitner(current_box: i64, correct: bool) -> (i64, i64) {
    let new_box = if correct {
        (current_box.clamp(1, MAX_BOX) + 1).min(MAX_BOX)
    } else {
        1
    };
    (new_box, INTERVAL_DAYS[(new_box - 1) as usize] * 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_climbs_one_box_and_caps_at_five() {
        assert_eq!(leitner(1, true), (2, 86_400));
        assert_eq!(leitner(2, true), (3, 3 * 86_400));
        assert_eq!(leitner(3, true), (4, 7 * 86_400));
        assert_eq!(leitner(4, true), (5, 21 * 86_400));
        assert_eq!(leitner(5, true), (5, 21 * 86_400));
    }

    #[test]
    fn wrong_falls_to_box_one_due_immediately() {
        for b in 1..=5 {
            assert_eq!(leitner(b, false), (1, 0));
        }
    }

    #[test]
    fn garbage_box_from_db_is_clamped() {
        assert_eq!(leitner(0, true), (2, 86_400));
        assert_eq!(leitner(99, true), (5, 21 * 86_400));
    }
}
