//! Static evaluation: the shape classes each side has, counted per direction.
//! Weights are untuned; positional terms (e.g. centrality) are not included yet.

use crate::bitboard::{Bits, Board};
use crate::threats::{SHAPES, Shapes};

/// Evaluations stay within +-MAX so they never collide with mate scores.
pub const MAX: i32 = 10_000;

/// Position value of each shape class for the side owning it, strongest first: five, open four,
/// four, open three, blocked three, two. Untuned.
const VALUE: [i32; SHAPES] = [3000, 1500, 300, 100, 25, 8];

fn shape_score<B: Bits>(shapes: &Shapes<B>, player: usize) -> i32 {
    let mut total = 0;
    for dir in shapes.ranked(player) {
        for (class, value) in VALUE.iter().enumerate() {
            total += value * dir[class].count_ones() as i32;
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
