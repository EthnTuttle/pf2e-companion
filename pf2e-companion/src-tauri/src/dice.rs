//! PF2e dice notation parser + roller (Phase 10 § 1.10).
//!
//! Supports the common PF2e expression grammar:
//!   `<count>d<sides>[k<keep>][+|-<modifier>]` — e.g. `2d6+3`, `d20`,
//!   `4d6k3`, `1d12-2`. Bounds: 1..=100 dice, 2..=1000 sides.
//!
//! Entropy comes from `uuid::Uuid::new_v4()` bytes (the `rand` crate is
//! outside the phase 10 dependency budget). Each die uses rejection
//! sampling over an 8-byte little-endian window so the modulo mapping is
//! unbiased: a draw `r` is accepted only when `r < u64::MAX - (u64::MAX %
//! sides)`, otherwise fresh entropy is drawn.

use std::fmt;

/// Parser / bounds errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiceError {
    Empty,
    Malformed(String),
    DiceCountOutOfBounds(u32),
    SidesOutOfBounds(u32),
    KeepLargerThanCount { keep: u32, count: u32 },
}

impl fmt::Display for DiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiceError::Empty => write!(f, "empty dice expression"),
            DiceError::Malformed(s) => write!(f, "malformed dice expression `{s}`"),
            DiceError::DiceCountOutOfBounds(n) => {
                write!(f, "dice count {n} outside the supported 1..=100 range")
            }
            DiceError::SidesOutOfBounds(n) => {
                write!(f, "die sides {n} outside the supported 2..=1000 range")
            }
            DiceError::KeepLargerThanCount { keep, count } => {
                write!(f, "keep {keep} larger than dice count {count}")
            }
        }
    }
}

impl std::error::Error for DiceError {}

/// A parsed dice expression. `keep` retains the highest N rolls
/// (`4d6k3` = roll 4, keep the best 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollSpec {
    pub count: u32,
    pub sides: u32,
    pub modifier: i32,
    pub keep: Option<u32>,
}

/// The result of rolling a [`RollSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollOutcome {
    pub expr: String,
    pub total: i64,
    pub rolls: Vec<u32>,
    pub kept: Vec<u32>,
}

/// Maximum dice per expression (contract § 2).
pub const MAX_DICE: u32 = 100;
/// Supported die-side range.
pub const MIN_SIDES: u32 = 2;
pub const MAX_SIDES: u32 = 1000;

/// Parse a PF2e dice expression. Whitespace is ignored; both cases of
/// `d`/`D` and `k`/`K` are accepted.
pub fn parse(expr: &str) -> Result<RollSpec, DiceError> {
    let s: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return Err(DiceError::Empty);
    }
    let s = s.to_ascii_lowercase();
    let malformed = || DiceError::Malformed(expr.to_string());

    // Non-ASCII input would make the byte-slicing below panic on a
    // multi-byte char boundary (e.g. "ḋ6") — reject it up front.
    if !s.is_ascii() {
        return Err(malformed());
    }

    // Split off the trailing modifier, if any.
    let (dice_part, modifier) = match s[1..].find(['+', '-']) {
        Some(idx) => {
            let split = idx + 1;
            let mod_str = &s[split..];
            let value: i32 = mod_str.parse().map_err(|_| malformed())?;
            (&s[..split], value)
        }
        None => (s.as_str(), 0),
    };

    // Split off the keep clause, if any.
    let (count_sides, keep) = match dice_part.find('k') {
        Some(idx) => {
            let keep_str = &dice_part[idx + 1..];
            let k: u32 = keep_str.parse().map_err(|_| malformed())?;
            (&dice_part[..idx], Some(k))
        }
        None => (dice_part, None),
    };

    // `<count>d<sides>` — count may be omitted (defaults to 1).
    let d_idx = count_sides.find('d').ok_or_else(malformed)?;
    let (count_str, sides_str) = (&count_sides[..d_idx], &count_sides[d_idx + 1..]);
    if sides_str.is_empty() {
        return Err(malformed());
    }
    let count: u32 = if count_str.is_empty() {
        1
    } else {
        count_str.parse().map_err(|_| malformed())?
    };
    let sides: u32 = sides_str.parse().map_err(|_| malformed())?;

    if !(1..=MAX_DICE).contains(&count) {
        return Err(DiceError::DiceCountOutOfBounds(count));
    }
    if !(MIN_SIDES..=MAX_SIDES).contains(&sides) {
        return Err(DiceError::SidesOutOfBounds(sides));
    }
    if let Some(k) = keep {
        if k < 1 || k > count {
            return Err(DiceError::KeepLargerThanCount { keep: k, count });
        }
    }

    Ok(RollSpec {
        count,
        sides,
        modifier,
        keep,
    })
}

/// Canonical rendering of a spec (`4d6k3+1`; a zero modifier is omitted).
pub fn render(spec: &RollSpec) -> String {
    let mut out = format!("{}d{}", spec.count, spec.sides);
    if let Some(k) = spec.keep {
        out.push('k');
        out.push_str(&k.to_string());
    }
    if spec.modifier != 0 {
        if spec.modifier > 0 {
            out.push('+');
        }
        out.push_str(&spec.modifier.to_string());
    }
    out
}

/// Roll the spec. Entropy: `uuid` v4 bytes with rejection sampling —
/// no `rand` dependency (contract § 1.10).
pub fn roll(spec: &RollSpec) -> RollOutcome {
    let mut rolls = Vec::with_capacity(spec.count as usize);
    for _ in 0..spec.count {
        rolls.push(roll_die(spec.sides));
    }
    // Keep the highest N dice, presented in descending order.
    let kept: Vec<u32> = match spec.keep {
        Some(k) if (k as usize) < rolls.len() => {
            let mut sorted = rolls.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            sorted.truncate(k as usize);
            sorted
        }
        _ => rolls.clone(),
    };
    let total: i64 = kept.iter().map(|&v| v as i64).sum::<i64>() + spec.modifier as i64;
    RollOutcome {
        expr: render(spec),
        total,
        rolls,
        kept,
    }
}

/// Draw one uniform value in `1..=sides` from uuid entropy, rejecting
/// draws that would bias the modulo mapping.
fn roll_die(sides: u32) -> u32 {
    let n = sides as u64;
    // Largest multiple of n that fits in u64 — draws at or above this are
    // rejected so every residue class is equally likely.
    let limit = u64::MAX - (u64::MAX % n);
    loop {
        let uuid = uuid::Uuid::new_v4();
        for chunk in uuid.as_bytes().chunks_exact(8) {
            let r = u64::from_le_bytes(chunk.try_into().expect("8-byte chunk"));
            if r < limit {
                return (r % n) as u32 + 1;
            }
            // Rejected — try the next window / a fresh uuid.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_forms() {
        assert_eq!(
            parse("2d6+3").unwrap(),
            RollSpec { count: 2, sides: 6, modifier: 3, keep: None }
        );
        assert_eq!(
            parse("d20").unwrap(),
            RollSpec { count: 1, sides: 20, modifier: 0, keep: None }
        );
        assert_eq!(
            parse("4d6k3").unwrap(),
            RollSpec { count: 4, sides: 6, modifier: 0, keep: Some(3) }
        );
        assert_eq!(
            parse("1D12-2").unwrap(),
            RollSpec { count: 1, sides: 12, modifier: -2, keep: None }
        );
        assert_eq!(
            parse(" 2 d 6 + 3 ").unwrap(),
            RollSpec { count: 2, sides: 6, modifier: 3, keep: None }
        );
    }

    #[test]
    fn rejects_bad_expressions() {
        assert_eq!(parse("").unwrap_err(), DiceError::Empty);
        assert!(matches!(parse("hello").unwrap_err(), DiceError::Malformed(_)));
        assert!(matches!(parse("2d").unwrap_err(), DiceError::Malformed(_)));
        assert!(matches!(parse("d6+").unwrap_err(), DiceError::Malformed(_)));
        assert!(matches!(parse("xd6").unwrap_err(), DiceError::Malformed(_)));
        assert_eq!(
            parse("0d6").unwrap_err(),
            DiceError::DiceCountOutOfBounds(0)
        );
        assert_eq!(
            parse("101d6").unwrap_err(),
            DiceError::DiceCountOutOfBounds(101)
        );
        assert_eq!(parse("2d1").unwrap_err(), DiceError::SidesOutOfBounds(1));
        assert_eq!(parse("2d1001").unwrap_err(), DiceError::SidesOutOfBounds(1001));
        assert_eq!(
            parse("3d6k4").unwrap_err(),
            DiceError::KeepLargerThanCount { keep: 4, count: 3 }
        );
    }

    #[test]
    fn roll_bounds_hold() {
        for expr in ["2d6+3", "d20", "4d6k3", "100d6"] {
            let spec = parse(expr).unwrap();
            let out = roll(&spec);
            assert_eq!(out.rolls.len(), spec.count as usize, "{expr}");
            for r in &out.rolls {
                assert!((1..=spec.sides).contains(r), "{expr}: roll {r} out of range");
            }
            let kept_sum: i64 = out.kept.iter().map(|&v| v as i64).sum();
            assert_eq!(out.total, kept_sum + spec.modifier as i64, "{expr}");
        }
    }

    #[test]
    fn keep_rule_retains_highest() {
        let spec = parse("4d6k3").unwrap();
        for _ in 0..20 {
            let out = roll(&spec);
            assert_eq!(out.kept.len(), 3);
            let mut sorted = out.rolls.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            assert_eq!(out.kept, &sorted[..3]);
            // Total is bounded by 3..=18.
            assert!((3..=18).contains(&out.total));
        }
    }

    #[test]
    fn modifier_applies() {
        let spec = parse("1d6+3").unwrap();
        let out = roll(&spec);
        assert_eq!(out.total, out.kept[0] as i64 + 3);
        assert_eq!(out.expr, "1d6+3");
    }
}
