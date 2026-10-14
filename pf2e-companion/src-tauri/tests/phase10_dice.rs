//! Phase 10 — dice notation parser/roller (CONTRACT § 6).
//!
//! Covers: "2d6+3" parse/roll bounds; "4d6k3" keep rule; bad-expression
//! errors; the 100-die cap; and the no-`rand` entropy path.

use pf2e_companion_lib::dice::{self, DiceError};

#[test]
fn parses_two_d_six_plus_three() {
    let spec = dice::parse("2d6+3").unwrap();
    assert_eq!(spec.count, 2);
    assert_eq!(spec.sides, 6);
    assert_eq!(spec.modifier, 3);
    assert_eq!(spec.keep, None);

    let out = dice::roll(&spec);
    assert_eq!(out.rolls.len(), 2);
    for r in &out.rolls {
        assert!((1..=6).contains(r), "roll {r} out of 1..=6");
    }
    assert_eq!(out.total, out.rolls.iter().map(|&v| v as i64).sum::<i64>() + 3);
    assert_eq!(out.expr, "2d6+3");
    // 2d6+3 ∈ [5, 15]
    assert!((5..=15).contains(&out.total), "total {} out of bounds", out.total);
}

#[test]
fn d20_defaults_to_one_die() {
    let spec = dice::parse("d20").unwrap();
    assert_eq!(spec.count, 1);
    assert_eq!(spec.sides, 20);
    let out = dice::roll(&spec);
    assert_eq!(out.rolls.len(), 1);
    assert!((1..=20).contains(&out.rolls[0]));
    assert_eq!(out.total as u32, out.rolls[0]);
}

#[test]
fn keep_rule_keeps_highest() {
    let spec = dice::parse("4d6k3").unwrap();
    assert_eq!(spec.keep, Some(3));
    for _ in 0..25 {
        let out = dice::roll(&spec);
        assert_eq!(out.rolls.len(), 4);
        assert_eq!(out.kept.len(), 3);
        let mut sorted = out.rolls.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(out.kept, &sorted[..3], "kept must be the highest 3");
        // 4d6k3 ∈ [3, 18]
        assert!((3..=18).contains(&out.total));
    }
}

#[test]
fn hundred_die_cap_enforced() {
    assert!(dice::parse("100d6").is_ok());
    let out = dice::roll(&dice::parse("100d6").unwrap());
    assert_eq!(out.rolls.len(), 100);
    assert_eq!(
        dice::parse("101d6").unwrap_err(),
        DiceError::DiceCountOutOfBounds(101)
    );
}

#[test]
fn rejects_bad_expressions() {
    assert_eq!(dice::parse("").unwrap_err(), DiceError::Empty);
    assert_eq!(dice::parse("   ").unwrap_err(), DiceError::Empty);
    for bad in ["hello", "2d", "d", "d+", "2d6++3", "xd6", "2d6x", "-3"] {
        assert!(
            matches!(dice::parse(bad).unwrap_err(), DiceError::Malformed(_)),
            "`{bad}` should be malformed"
        );
    }
    // Non-ASCII input (multi-byte first char used to panic the parser's
    // byte slicing) is rejected, not crashed on.
    for bad in ["ḋ6", "2ḓ6", "🎲6", "1d20＋5"] {
        assert!(
            matches!(dice::parse(bad).unwrap_err(), DiceError::Malformed(_)),
            "`{bad}` (non-ASCII) should be malformed, not panic"
        );
    }
    assert_eq!(dice::parse("2d1").unwrap_err(), DiceError::SidesOutOfBounds(1));
    assert_eq!(
        dice::parse("2d1001").unwrap_err(),
        DiceError::SidesOutOfBounds(1001)
    );
    assert_eq!(
        dice::parse("3d6k4").unwrap_err(),
        DiceError::KeepLargerThanCount { keep: 4, count: 3 }
    );
    assert_eq!(
        dice::parse("3d6k0").unwrap_err(),
        DiceError::KeepLargerThanCount { keep: 0, count: 3 }
    );
}

#[test]
fn negative_modifier_parses() {
    let spec = dice::parse("1d12-2").unwrap();
    assert_eq!(spec.modifier, -2);
    let out = dice::roll(&spec);
    assert_eq!(out.total, out.rolls[0] as i64 - 2);
    // 1d12-2 ∈ [-1, 10]
    assert!((-1..=10).contains(&out.total));
}

#[test]
fn entropy_is_rejection_sampled_and_bounded() {
    // A long run over many shapes must never leave the die range — the
    // rejection sampler must terminate and stay in-bounds.
    for sides in [2u32, 3, 4, 6, 8, 10, 12, 20, 100, 1000] {
        let spec = dice::parse(&format!("50d{sides}")).unwrap();
        let out = dice::roll(&spec);
        assert!(
            out.rolls.iter().all(|&r| (1..=sides).contains(&r)),
            "side {sides}: out of range rolls {:?}",
            out.rolls
        );
    }
}
