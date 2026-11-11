//! Static evaluation: the shape classes each side has, counted per direction.
//! Weights are untuned; positional terms (e.g. centrality) are not included yet.

use crate::bitboard::{Bits, Board, DIRS};
use crate::threats::{SHAPES, Shapes};

/// Evaluations stay within +-MAX so they never collide with mate scores.
pub const MAX: i32 = 10_000;

/// Position value of each shape class for the side owning it, strongest first: five, open four,
/// four, open three, blocked three, two.
///
/// Fitted to Rapfi's depth-8 score (`engine/scripts/fit_eval.py`, KANEC pool, read on the 35 games held out
/// of the fit): r 0.433 -> 0.712, replacing `[3240, 1620, 162, 162, 27, 9]`. The five keeps its old value
/// (no corpus position lets either side complete five) and the scale is unchanged, so `futility_margin`
/// still holds.
///
/// It barely matters in tactical games and decides long ones: +2 games in 520 over the five-leg pool and
/// level against KANEC (72W-31L against 66W-38L, and a deliberately inverted open-four/four read 67W-30L),
/// but against pela the decided games go 52.5% -> 64.5% and 67.4% in two runs and the 61+ move bucket
/// 18% -> 33% (the inverted control: 37.9% and 2%).
///
/// Additions on top were tried and rejected. A double-threat term scored 61.7% on its own with the shipped
/// values but is redundant with these: 60.4% together, and 0.692 in the fit against 0.712 alone. A
/// threat-direction term gave 56.3%. Fitting the values to *game outcomes* instead of to Rapfi's number
/// (`engine/scripts/fit_outcome.py`, 2294 games, held-out AUC 0.6734 against 0.6600 for these) predicted
/// outcomes slightly better and played clearly worse: 44.1% of the decided pela games against 52.5% for the
/// shipped values and 64.5% here. So the target was not the problem either. And the ladder's steepness was
/// swept past these values too: a step flatter (small classes up, the five carried with the rest so the
/// spread stays put) reads 0.708 against 0.692 on the pela corpus and plays 59.8% against 64.5% and 67.4%
/// here, so this point is the best of the three measured. Everything was measured on pela, the only opponent
/// that takes the game into the phase where the evaluation decides; the five-leg pool reads +2 games in 520.
const VALUE: [i32; SHAPES] = [3240, 540, 445, 140, 140, 104];

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
///
/// Public so `examples/eval_dump.rs` can dump these counts without a second copy.
pub fn groups<B: Bits>(m: B, d: u32) -> i32 {
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

/// Score from the point of view of `side`, which is the player to move. Classes are counted per direction,
/// so a stone that creates a threat in two directions counts twice, and a direction contributes only the
/// strongest shape the cell makes in it. The difference is halved, which keeps busy positions clear of `MAX`
/// without changing the order of two positions that differ by more than one point.
///
/// A ramp that steepened these values with the stone count was built on the pela corpus and withdrawn: split
/// by ply, the band-matched vectors lose to these flat values when they are read out of sample - late band
/// 0.609 against 0.645, early 0.708 against 0.710 - so the weights do not want to differ by phase.
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
