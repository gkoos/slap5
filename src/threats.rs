//! Threat masks over empty cells, from the point of view of the player owning `own`.
//! Freestyle rules: five or more in a row wins. Every mask is a subset of `empty`.
//!
//! The masks the forcing rules use (`five_cells`, `four_cells`, `open_four_cells`, ...) are unions
//! over the four directions. Move ordering and evaluation need more than that: the shape per
//! direction and how many directions show it, so that a stone making two threats counts twice.
//! `Shapes` gives that view for both players at once, as one of six classes per direction, ordered
//! by strength, with a cell belonging only to the strongest class it makes in that direction (an
//! open four is not also counted as a four).

use crate::bitboard::{Bits, Board, DIRS};

/// Classes of the shape a stone creates in one direction, strongest first.
pub const FIVE: usize = 0;
pub const OPEN_FOUR: usize = 1;
pub const FOUR: usize = 2;
pub const OPEN_THREE: usize = 3;
pub const BLOCKED_THREE: usize = 4;
pub const TWO: usize = 5;
/// Number of shape classes.
pub const SHAPES: usize = 6;

/// Bit s of `o[j]`/`e[j]` is the own/empty state of the cell j steps after cell s along `d`.
#[inline(always)]
fn lines<B: Bits>(own: B, empty: B, d: u32) -> ([B; 6], [B; 6]) {
    let mut o = [B::ZERO; 6];
    let mut e = [B::ZERO; 6];
    for j in 0..6 {
        o[j] = own.shr(j as u32 * d);
        e[j] = empty.shr(j as u32 * d);
    }
    (o, e)
}

/// Shape masks of `own` along `d`, one per class, before the class precedence: bit `s + k*d` of
/// class `c` means that playing at `s + k*d` creates shape `c` in the window starting at cell `s`.
/// A cell may appear in several classes here; `precedence` resolves that.
fn dir_shapes<B: Bits>(own: B, empty: B, d: u32) -> [B; SHAPES] {
    let (o, e) = lines(own, empty, d);
    let mut s = [B::ZERO; SHAPES];

    // Five: the other four cells of a five-cell window are own.
    for k in 0..5 {
        let mut m = e[k];
        for j in 0..5 {
            if j != k {
                m = m.and(o[j]);
            }
        }
        s[FIVE] = s[FIVE].or(m.shl(k as u32 * d));
    }

    // Open four: a six-cell window with empty ends and three own stones among its middle four; the
    // single empty middle cell completes a run of exactly four with both ends empty.
    for k in 1..5 {
        let mut m = e[0].and(e[5]).and(e[k]);
        for j in 1..5 {
            if j != k {
                m = m.and(o[j]);
            }
        }
        s[OPEN_FOUR] = s[OPEN_FOUR].or(m.shl(k as u32 * d));
    }

    // Four: a five-cell window with three own stones and two empty cells; playing either empty cell
    // makes four in a row and leaves one five-completing cell.
    for k in 0..5 {
        for hole in 0..5 {
            if hole == k {
                continue;
            }
            let mut m = e[k].and(e[hole]);
            for j in 0..5 {
                if j != k && j != hole {
                    m = m.and(o[j]);
                }
            }
            s[FOUR] = s[FOUR].or(m.shl(k as u32 * d));
        }
    }

    // Open three: a six-cell window with empty ends and two own plus two empty middle cells; both
    // empty middle cells make an open four.
    for k in 1..5 {
        for l in (k + 1)..5 {
            let mut m = e[0].and(e[5]).and(e[k]).and(e[l]);
            for j in 1..5 {
                if j != k && j != l {
                    m = m.and(o[j]);
                }
            }
            s[OPEN_THREE] = s[OPEN_THREE].or(m.shl(k as u32 * d)).or(m.shl(l as u32 * d));
        }
    }

    // Blocked three and two: a five-cell window with no opponent and no off-board cell holding
    // exactly two own stones (three) or one (two); every empty cell of it gains that shape.
    let (mut free, mut ge1, mut ge2, mut ge3) = (o[0].or(e[0]), o[0], B::ZERO, B::ZERO);
    for j in 1..5 {
        free = free.and(o[j].or(e[j]));
        ge3 = ge3.or(ge2.and(o[j]));
        ge2 = ge2.or(ge1.and(o[j]));
        ge1 = ge1.or(o[j]);
    }
    let holds_two = free.and(ge2).and(ge3.not());
    let holds_one = free.and(ge1).and(ge2.not());
    for k in 0..5 {
        s[BLOCKED_THREE] = s[BLOCKED_THREE].or(holds_two.and(e[k]).shl(k as u32 * d));
        s[TWO] = s[TWO].or(holds_one.and(e[k]).shl(k as u32 * d));
    }

    s
}

/// Every class minus all stronger ones, so a cell belongs to one class per direction.
fn precedence<B: Bits>(mut s: [B; SHAPES]) -> [B; SHAPES] {
    let mut stronger = s[FIVE];
    for class in OPEN_FOUR..SHAPES {
        s[class] = s[class].and(stronger.not());
        stronger = stronger.or(s[class]);
    }
    s
}

/// Shape masks of both players at one position: `[player][direction][class]`.
///
/// `ranked` has the class precedence applied and is what move ordering and evaluation read. `raw`
/// is the same without it; the union over the directions of one raw class is the set the forcing
/// rules have always used, so `cells` answers what the union functions below answer.
#[derive(Clone, Copy)]
pub struct Shapes<B: Bits> {
    raw: [[[B; SHAPES]; 4]; 2],
    ranked: [[[B; SHAPES]; 4]; 2],
}

impl<B: Bits> Shapes<B> {
    /// Builds the masks of both players for one position.
    pub fn new(b: &Board<B>) -> Self {
        let empty = b.empty();
        let mut raw = [[[B::ZERO; SHAPES]; 4]; 2];
        let mut ranked = [[[B::ZERO; SHAPES]; 4]; 2];
        for player in 0..2 {
            for (i, d) in DIRS.iter().enumerate() {
                raw[player][i] = dir_shapes(b.stones[player], empty, *d);
                ranked[player][i] = precedence(raw[player][i]);
            }
        }
        Shapes { raw, ranked }
    }

    /// The per-direction classes of `player`, after the precedence.
    #[inline(always)]
    pub fn ranked(&self, player: usize) -> &[[B; SHAPES]; 4] {
        &self.ranked[player]
    }

    /// Cells where playing creates this shape for `player` in any direction.
    pub fn cells(&self, player: usize, class: usize) -> B {
        let mut res = B::ZERO;
        for d in 0..4 {
            res = res.or(self.raw[player][d][class]);
        }
        res
    }
}

/// Union over the four directions of one class, before the precedence.
fn union<B: Bits>(own: B, empty: B, class: usize) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        res = res.or(dir_shapes(own, empty, d)[class]);
    }
    res
}

/// Cells where placing a stone makes five or more in a row.
pub fn five_cells<B: Bits>(own: B, empty: B) -> B {
    union(own, empty, FIVE)
}

/// Cells where placing a stone leaves a window of five with four own stones and one empty cell.
pub fn four_cells<B: Bits>(own: B, empty: B) -> B {
    union(own, empty, FOUR)
}

/// Cells where placing a stone makes a run of exactly four with both ends empty.
pub fn open_four_cells<B: Bits>(own: B, empty: B) -> B {
    union(own, empty, OPEN_FOUR)
}

/// Cells where placing a stone makes an open four possible next move using that stone:
/// a six-cell window with empty ends whose middle four hold two own stones and two empty cells.
pub fn open_three_cells<B: Bits>(own: B, empty: B) -> B {
    union(own, empty, OPEN_THREE)
}

/// Both ends and the middle gap of every window `open_four_cells` would use. A superset of the
/// replies that stop an open four: a far end can be listed although taking it does not help.
pub fn open_four_block_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        let ends = e[0].and(e[5]);
        for k in 1..=4 {
            let mut m = ends.and(e[k]);
            for j in 1..=4 {
                if j != k {
                    m = m.and(o[j]);
                }
            }
            res = res.or(m).or(m.shl(5 * d)).or(m.shl(k as u32 * d));
        }
    }
    res
}

/// Cells where placing a stone leaves a window of five with three own stones and two empty cells, and
/// cells where it leaves a window with two own stones and three empty cells.
pub fn three_and_two_cells<B: Bits>(own: B, empty: B) -> (B, B) {
    (union(own, empty, BLOCKED_THREE), union(own, empty, TWO))
}

impl<B: Bits> Board<B> {
    /// Shape masks of both players, for move ordering and evaluation.
    pub fn shapes(&self) -> Shapes<B> {
        Shapes::new(self)
    }

    pub fn three_and_two_cells(&self, player: usize) -> (B, B) {
        three_and_two_cells(self.stones[player], self.empty())
    }

    pub fn five_cells(&self, player: usize) -> B {
        five_cells(self.stones[player], self.empty())
    }

    pub fn four_cells(&self, player: usize) -> B {
        four_cells(self.stones[player], self.empty())
    }

    pub fn open_four_cells(&self, player: usize) -> B {
        open_four_cells(self.stones[player], self.empty())
    }

    pub fn open_three_cells(&self, player: usize) -> B {
        open_three_cells(self.stones[player], self.empty())
    }

    pub fn open_four_block_cells(&self, player: usize) -> B {
        open_four_block_cells(self.stones[player], self.empty())
    }
}

#[cfg(test)]
mod tests;
