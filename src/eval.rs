//! Placeholder static evaluation: weighted counts of the cells where each side could make a threat.
//! Weights are untuned; positional terms (e.g. centrality) are not included yet.

use crate::bitboard::{Bits, Board};

/// Evaluations stay within +-MAX so they never collide with mate scores.
pub const MAX: i32 = 10_000;

const W_FIVE: i32 = 3000;
const W_OPEN_FOUR: i32 = 1500;
const W_FOUR: i32 = 300;
const W_OPEN_THREE: i32 = 100;

fn threat_score<B: Bits>(b: &Board<B>, player: usize) -> i32 {
    W_FIVE * b.five_cells(player).count_ones() as i32
        + W_OPEN_FOUR * b.open_four_cells(player).count_ones() as i32
        + W_FOUR * b.four_cells(player).count_ones() as i32
        + W_OPEN_THREE * b.open_three_cells(player).count_ones() as i32
}

/// Score from the point of view of `side`, which is the player to move.
pub fn evaluate<B: Bits>(b: &Board<B>, side: usize) -> i32 {
    (threat_score(b, side) - threat_score(b, 1 - side)).clamp(-MAX, MAX)
}

#[cfg(test)]
mod tests;
