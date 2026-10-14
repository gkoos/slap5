//! Threat masks over empty cells, from the point of view of the player owning `own`.
//! Freestyle rules: five or more in a row wins. Every mask is a subset of `empty`.

use crate::bitboard::{Bits, Board, DIRS};

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

/// Cells where placing a stone makes five or more in a row.
pub fn five_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        for k in 0..5 {
            let mut m = e[k];
            for j in 0..5 {
                if j != k {
                    m = m.and(o[j]);
                }
            }
            res = res.or(m.shl(k as u32 * d));
        }
    }
    res
}

/// Cells where placing a stone leaves a window of five with four own stones and one empty cell.
pub fn four_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
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
                res = res.or(m.shl(k as u32 * d));
            }
        }
    }
    res
}

/// Cells where placing a stone makes a run of exactly four with both ends empty.
pub fn open_four_cells<B: Bits>(own: B, empty: B) -> B {
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
            res = res.or(m.shl(k as u32 * d));
        }
    }
    res
}

/// Cells where placing a stone makes an open four possible next move using that stone:
/// a six-cell window with empty ends whose middle four hold two own stones and two empty cells.
pub fn open_three_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        let ends = e[0].and(e[5]);
        for k in 1..=4 {
            for l in (k + 1)..=4 {
                let mut m = ends.and(e[k]).and(e[l]);
                for j in 1..=4 {
                    if j != k && j != l {
                        m = m.and(o[j]);
                    }
                }
                res = res.or(m.shl(k as u32 * d)).or(m.shl(l as u32 * d));
            }
        }
    }
    res
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
    let (mut three, mut two) = (B::ZERO, B::ZERO);
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        // Windows starting at each cell with no opponent stone and no off-board cell, by own stones held.
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
            three = three.or(holds_two.and(e[k]).shl(k as u32 * d));
            two = two.or(holds_one.and(e[k]).shl(k as u32 * d));
        }
    }
    (three, two)
}

impl<B: Bits> Board<B> {
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
