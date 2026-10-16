//! Static evaluation: the shape classes each side has, counted per direction.
//! Weights are untuned; positional terms (e.g. centrality) are not included yet.

use crate::bitboard::{Bits, Board, DIRS};
use crate::threats::{SHAPES, Shapes};

/// Evaluations stay within +-MAX so they never collide with mate scores.
pub const MAX: i32 = 10_000;

/// Position value of each shape class for the side owning it, strongest first: five, open four,
/// four, open three, blocked three, two.
///
/// The four-to-open-three ratio is 1:1 here, not the 3:1 it started at. The corpus chose the direction:
/// with the residual being search minus static, the fours feature (ours minus theirs) correlated -0.672
/// with it and open threes +0.574 over 90 positions, so the eval was overpricing its own fours and
/// underpricing its own open threes. That matches the game - a four is a one-move threat that is trivially
/// answered, an open three is the live one that wins positions. The other four classes, and the overall
/// scale, are untouched; `scripts/retune.ps1` is what checks the scale by node count at equal depth.
///
/// Then the whole set is scaled 0.9, because the ratio change moved the average magnitude up - node count
/// at equal depth read 1.122 of the shipped revision, i.e. the search pruned 12% less than shipped - and
/// the pruning thresholds are only meaningful at the scale the eval had when they were tuned by games.
/// 1:1 four-to-open-three with the overall scale back at parity is the configuration to test.
const VALUE: [i32; SHAPES] = [3240, 1620, 162, 162, 27, 9];

/// A side-to-move term. The eval is scored from the mover's point of view, and the corpus says the two
/// colours are not priced the same: on 541 sampled game positions the mean residual (search minus static)
/// was **+578 with black to move against +487 with white**, with effectively identical spreads (sd 571 and
/// 564), and a smaller sample agreed in direction (+126). Half that gap per side closes it - black +45,
/// white -45. Unlike a global offset this does not cancel, because it changes the value of every line by a
/// constant that depends on whose turn it is, which is what a tempo term is. Two caveats are attached: the
/// corpus sampled only black-to-move positions until its parity grid was fixed, so this evidence is a day
/// old, and only a match can ratify a number the search's own scores helped define.
const TEMPO: i32 = 45;

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
    (diff / 2 + if side == 0 { TEMPO } else { -TEMPO }).clamp(-MAX, MAX)
}

/// Score from the point of view of `side`, for callers that hold only a board.
pub fn evaluate<B: Bits>(b: &Board<B>, side: usize) -> i32 {
    evaluate_shapes(&b.shapes(), side)
}

#[cfg(test)]
mod tests;
