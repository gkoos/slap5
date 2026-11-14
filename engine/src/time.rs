//! The thinking budget for one move, from the Gomocup time controls.
//!
//! Piskvork re-sends `INFO time_left` before every turn (Piskvork `protocol.cpp:776`), so the value
//! handed in here is always the remaining match time for *this* turn; nothing needs to be
//! accumulated across moves. `timeout_turn` is the per-move cap, and 0 is the protocol's "as fast
//! as possible".
//!
//! The shape is adopted from the reference engines, not fitted by us:
//! - Carbon and the Piskvork reference brain divide the match clock by 7 "spare moves":
//!   `min(timeout_turn, time_left/7) - overhead`.
//! - PentaZen reserves 10 s off the clock and divides the rest by 6, with a 50 ms overhead.
//! - Rapfi divides by `min(7, movesToGo)` and keeps a 30 ms overhead.
//!
//! We keep PentaZen's 10 s reserve combined with the Carbon/reference/Rapfi divisor of 7, both on
//! the conservative side. The reserve is what keeps a long game safe at the end of the match clock -
//! the previous `time_left/10` placeholder front-loaded the clock and then starved the endgame, and
//! it kept no reserve at all.

use std::time::Duration;

/// Milliseconds kept back from the match clock so it cannot be run down late in a long game.
const RESERVE_MS: i64 = 10_000;
/// "Moves spared for the rest of the game": the reserve-adjusted clock is divided by this.
const SPARE_MOVES: i64 = 7;
/// One-time overhead for pipe latency and the search's 1024-node deadline-check granularity.
const OVERHEAD_MS: i64 = 50;
/// The budget never drops below this, so a reply is always produced.
const FLOOR_MS: i64 = 20;

/// How long to think, in milliseconds, given the current protocol time controls.
pub fn budget(timeout_turn: Option<u64>, time_left: Option<i64>) -> Duration {
    // 0 is the protocol's "as fast as possible". A missing value keeps a 1 s default so a human
    // driving the binary without an INFO still gets a usable move.
    let turn_ms = match timeout_turn {
        Some(0) => FLOOR_MS,
        Some(t) => t as i64,
        None => 1_000,
    };
    // The share of the reserve-adjusted match clock this move may take. Without `time_left` the
    // clock does not bind at all.
    let clock_ms = match time_left {
        Some(left) => left.saturating_sub(RESERVE_MS).max(0) / SPARE_MOVES,
        None => i64::MAX,
    };
    Duration::from_millis((turn_ms.min(clock_ms) - OVERHEAD_MS).max(FLOOR_MS) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_info_keeps_the_one_second_default() {
        assert_eq!(budget(None, None), Duration::from_millis(950));
    }

    #[test]
    fn turn_cap_is_honoured_without_a_match_clock() {
        assert_eq!(budget(Some(1000), None), Duration::from_millis(950));
    }

    #[test]
    fn zero_turn_means_as_fast_as_possible() {
        assert_eq!(budget(Some(0), None), Duration::from_millis(20));
        assert_eq!(budget(Some(0), Some(180_000)), Duration::from_millis(20));
    }

    #[test]
    fn match_clock_share_is_reserve_adjusted_then_divided() {
        // (180000 - 10000) / 7 = 24285, then - 50.
        assert_eq!(budget(Some(30_000), Some(180_000)), Duration::from_millis(24_235));
        // (1000000 - 10000) / 7 = 141428 exceeds the turn cap, so the turn binds.
        assert_eq!(budget(Some(5_000), Some(1_000_000)), Duration::from_millis(4_950));
    }

    #[test]
    fn an_exhausted_clock_answers_immediately() {
        // 2 s left is inside the 10 s reserve, so the share is zero and the floor binds.
        assert_eq!(budget(Some(30_000), Some(2_000)), Duration::from_millis(20));
        assert_eq!(budget(Some(30_000), Some(-500)), Duration::from_millis(20));
    }

    #[test]
    fn the_unlimited_sentinel_does_not_bind() {
        // Piskvork sends 0x7fffffff when the match time is unlimited; the turn cap must bind.
        assert_eq!(budget(Some(1000), Some(2_147_483_647)), Duration::from_millis(950));
    }
}