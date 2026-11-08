//! A VCF prover, following PentaZen: called at the horizon, budgeted by a depth, cut to two attacker
//! moves below the top, candidates ordered as their generator orders them, a bucketed table that
//! returns the best move as well as the value, and only ever a win for a sequence walked to a five.

use crate::bitboard::{Bits, STRIDE};
use crate::search::MATE;
use crate::threats;
use crate::zobrist;
use crate::Board;
use std::time::Instant;

const SLOTS: usize = 1 << 12;
const BUCKET: usize = 4;

#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    best: u8,
    dist: u16,
    depth: i8,
}

/// What the walk may spend, and what it remembers.
pub struct Budget {
    pub nodes: i32,
    deadline: Option<Instant>,
    entries: Vec<Entry>,
}

impl Budget {
    pub fn new(nodes: i32) -> Self {
        let empty = Entry { key: 0, best: 0, dist: 0, depth: -1 };
        Budget { nodes, deadline: None, entries: vec![empty; SLOTS * BUCKET] }
    }

    #[inline]
    /// The engine hands the walk its turn deadline: a proof that cannot finish inside the turn is
    /// no proof, and the node budget alone does not bound the time.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    #[inline]
    fn spend(&mut self) -> bool {
        if self.nodes & 1023 == 0 && let Some(d) = self.deadline && Instant::now() >= d {
            self.nodes = 0;
        }
        self.nodes -= 1;
        self.nodes >= 0
    }

    /// (plies to the five, depth it was proved at, best move). `dist == 0` means proved no-win here.
    pub fn probe(&self, key: u64) -> Option<(u16, i8, u8)> {
        let base = (key as usize & (SLOTS - 1)) * BUCKET;
        for i in base..base + BUCKET {
            if self.entries[i].key == key {
                return Some((self.entries[i].dist, self.entries[i].depth, self.entries[i].best));
            }
        }
        None
    }

    fn store(&mut self, key: u64, best: u8, dist: u16, depth: i32) {
        let base = (key as usize & (SLOTS - 1)) * BUCKET;
        let d = if depth > 127 { 127 } else { depth as i8 };
        let mut victim = base;
        for i in base..base + BUCKET {
            if self.entries[i].key == key {
                victim = i;
                break;
            }
            if self.entries[i].key == 0 || self.entries[i].depth < self.entries[victim].depth {
                victim = i;
            }
        }
        self.entries[victim] = Entry { key, best, dist, depth: d };
    }
}

/// A proven win for `side` to move, or nothing. The score is `MATE - ply - k`.
pub fn vcf(b: &mut Board, side: usize, ply: usize, depth: i32, budget: &mut Budget) -> Option<i32> {
    walk(b, side, ply, depth, true, budget)
}

/// `wide` is true only at the top: a non-root node is cut to two attacker moves.
fn walk(b: &mut Board, side: usize, ply: usize, depth: i32, wide: bool, budget: &mut Budget) -> Option<i32> {
    if !budget.spend() || depth <= 0 {
        return None;
    }
    let key = b.hash() ^ if side == 1 { zobrist::SIDE } else { 0 };
    let mut stored = 0u8;
    if let Some((dist, at, best)) = budget.probe(key) {
        if at as i32 >= depth {
            if dist == 0 {
                return None;
            }
            return Some(MATE - ply as i32 - dist as i32);
        }
        stored = best;
    }
    let own = b.stones[side];
    let empty = b.empty();
    if !threats::five_cells(own, empty).is_zero() {
        return Some(MATE - ply as i32 - 1);
    }
    // Their gate: a blocked three exists. Ours adds fours and open threes, which the walk can use.
    // Candidate fours, the table move first, then open fours before blocked ones.
    let open = threats::open_four_cells(own, empty);
    let blocked = threats::four_cells(own, empty).and(open.not());
    let mut cells: Vec<usize> = open.bits().chain(blocked.bits()).collect();
    if stored != 0 {
        let want = stored as usize;
        if let Some(p) = cells.iter().position(|&c| c == want) {
            cells.swap(0, p);
        }
    }
    let mut tried = 0;
    for cell in cells {
        if !wide {
            if tried >= 2 {
                break;
            }
            tried += 1;
        }
        let (r, c) = (cell / STRIDE, cell % STRIDE);
        b.make(side, r, c);
        // Only a four that is a double four, or also makes a three (open or blocked), is walked.
        let after = b.stones[side];
        let after_empty = b.empty();
        if threats::four_cells(after, after_empty).is_zero()
            && threats::open_three_cells(after, after_empty).is_zero()
            && threats::five_cells(after, after_empty).count_ones() < 2
        {
            b.unmake(side, r, c);
            continue;
        }
        let value = answer(b, side, ply, depth, budget);
        b.unmake(side, r, c);
        if let Some(v) = value {
            budget.store(key, cell as u8, (MATE - v - ply as i32) as u16, depth);
            return Some(v);
        }
    }
    // Nothing here at this depth, and the budget did not run out: that is worth remembering.
    if budget.nodes > 0 {
        budget.store(key, 0, 0, depth);
    }
    None
}

/// The position after our four: the opponent is to move and their only non-losing answers are the
/// cells that complete it for us.
fn answer(b: &mut Board, side: usize, ply: usize, depth: i32, budget: &mut Budget) -> Option<i32> {
    let opp = 1 - side;
    let empty = b.empty();
    if !threats::five_cells(b.stones[opp], empty).is_zero() {
        return None;
    }
    let completions = threats::five_cells(b.stones[side], empty);
    if completions.count_ones() >= 2 {
        return Some(MATE - ply as i32 - 3);
    }
    let cell = completions.bits().next()?;
    let (r, c) = (cell / STRIDE, cell % STRIDE);
    b.make(opp, r, c);
    let value = if b.wins(opp) {
        None
    } else {
        walk(b, side, ply + 2, depth - 2, false, budget)
    };
    b.unmake(opp, r, c);
    value
}

#[cfg(test)]
mod tests;
