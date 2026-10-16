//! Static evaluation: the shape classes each side has, counted per direction.
//! Weights are untuned; positional terms (e.g. centrality) are not included yet.

use crate::bitboard::{Bits, Board, DIRS};
use crate::threats::{SHAPES, Shapes};

/// Evaluations stay within +-MAX so they never collide with mate scores.
pub const MAX: i32 = 10_000;

/// Position value of each shape class for the side owning it, strongest first: five, open four,
/// four, open three, blocked three, two.
///
/// Scaled by 1.2 when `shape_score` started counting each threat once instead of two to four times.
/// The classes' relative weights are unchanged - this is not a tuning - and the point is the scale:
/// `futility_margin` is in the same units, and `150 + 120·depth` is the value that won game blocks, so
/// the eval is put back where that measurement was taken. The factor is measured with
/// `scripts/retune.ps1` by node count at equal depth against the shipped revision: 0.936 of the old
/// build at 1.0, 1.070 at 1.45, so parity sits near 1.2. That script's other estimator, the corpus's
/// ordinary-position mean, is the weaker of the two - the set it averages over is defined by the
/// residual, and its membership shifts between runs.
const VALUE: [i32; SHAPES] = [3600, 1800, 360, 120, 30, 10];

/// Number of groups of adjacent cells along `d` in a mask of playable cells. The class masks hold the
/// cells where playing *creates* that shape, so a single threat - an open three, say - sets two or
/// three neighbouring cells at once, and counting the cells counts the shape several times. Counting
/// the groups counts the threat once, which is what the class value is supposed to mean.
fn groups<B: Bits>(m: B, d: u32) -> i32 {
    (m.count_ones() - m.and(m.shr(d)).count_ones()) as i32
}

fn shape_score<B: Bits>(shapes: &Shapes<B>, player: usize) -> i32 {
    let mut total = 0;
    for (i, d) in DIRS.iter().enumerate() {
        for (class, value) in VALUE.iter().enumerate() {
            total += value * groups(shapes.ranked(player)[i][class], *d);
        }
    }
    total
}

/// Score from the point of view of `side`, which is the player to move. Classes are counted per
/// direction, so a stone that creates a threat in two directions counts twice, and a direction
/// contributes only the strongest shape the cell makes in it. The difference is halved, which keeps
/// busy positions clear of `MAX` without changing the order of two positions that differ by more
/// than one point.
pub fn evaluate_shapes<B: Bits>(shapes: &Shapes<B>, side: usize) -> i32 {
    let diff = shape_score(shapes, side) - shape_score(shapes, 1 - side);
    (diff / 2).clamp(-MAX, MAX)
}

/// Score from the point of view of `side`, for callers that hold only a board.
pub fn evaluate<B: Bits>(b: &Board<B>, side: usize) -> i32 {
    evaluate_shapes(&b.shapes(), side)
}

#[cfg(test)]
mod tests;
