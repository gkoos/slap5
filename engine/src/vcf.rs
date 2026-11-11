//! A VCF prover, following PentaZen: called at the horizon, budgeted by a depth, cut to two attacker
//! moves below the top, candidates ordered as their generator orders them, a bucketed table that
//! returns the best move as well as the value, and only ever a win for a sequence walked to a five.

use crate::bitboard::{Bits, STRIDE};
use crate::clock::Instant;
use crate::search::MATE;
use crate::threats;
use crate::zobrist;
use crate::{Board, Words};

const SLOTS: usize = 1 << 12;
const BUCKET: usize = 4;

#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    best: u8,
    dist: u16,
    depth: i8,
}

/// An empty table slot. Buckets are scanned for a matching key, so a zero key is never stored.
const EMPTY: Entry = Entry { key: 0, best: 0, dist: 0, depth: -1 };

/// What the walk may spend, and what it remembers.
pub struct Budget {
    /// The budget still to spend, in nodes.
    pub nodes: i32,
    /// Nodes the walk actually entered, counted apart from the budget so a deadline cut is not
    /// reported as the whole budget spent (see `spend`).
    pub visited: i32,
    deadline: Option<Instant>,
    entries: Vec<Entry>,
}

impl Budget {
    pub fn new(nodes: i32) -> Self {
        Budget { nodes, visited: 0, deadline: None, entries: vec![EMPTY; SLOTS * BUCKET] }
    }

    /// Resets the counters and clears the table for a fresh turn. Reusing one `Budget` across turns
    /// saves the table allocation; clearing it keeps an entry from one turn from reaching the next,
    /// which would otherwise reach a later position and change an equivalence benchmark.
    #[inline]
    pub fn reset(&mut self, nodes: i32) {
        self.entries.fill(EMPTY);
        self.nodes = nodes;
        self.visited = 0;
    }

    #[inline]
    /// The engine hands the walk its turn deadline: a proof that cannot finish inside the turn is
    /// no proof, and the node budget alone does not bound the time.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    #[inline]
    /// Sets the deadline on an existing budget, for a table reused across turns (see `reset`).
    pub fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = Some(deadline);
    }

    #[inline]
    fn spend(&mut self) -> bool {
        self.visited += 1;
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

/// `wide` is true only at the top: a non-root node is cut to two attacker moves. The masks come from
/// `threats::vcf_masks`, which builds the four classes the prover reads in one pass per direction,
/// rather than the per-class generation that rebuilt all six classes for each one.
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
    // `five` proves the node; `open_four` and `four` are the candidate set, all from one pass.
    let m = threats::vcf_masks(b.stones[side], b.empty());
    if !m.five.is_zero() {
        return Some(MATE - ply as i32 - 1);
    }
    // Candidate fours: the table move first, then open fours before blocked ones.
    let open = m.open_four;
    let blocked = m.four.and(open.not());
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
        let ma = threats::vcf_masks(b.stones[side], b.empty());
        let five_after = ma.five;
        if five_after.count_ones() < 2 && ma.four.is_zero() && ma.open_three.is_zero() {
            b.unmake(side, r, c);
            continue;
        }
        let value = answer(b, side, ply, depth, five_after, budget);
        b.unmake(side, r, c);
        if let Some(v) = value {
            budget.store(key, cell as u8, (MATE - v - ply as i32) as u16, depth);
            return Some(v);
        }
    }
    if budget.nodes > 0 {
        budget.store(key, 0, 0, depth);
    }
    None
}

/// The position after our four: the opponent is to move and their only non-losing answers are the
/// cells that complete it for us. `completions` is our five mask, already built by the caller.
fn answer(b: &mut Board, side: usize, ply: usize, depth: i32, completions: Words, budget: &mut Budget) -> Option<i32> {
    let opp = 1 - side;
    let empty = b.empty();
    if !threats::five_cells(b.stones[opp], empty).is_zero() {
        return None;
    }
    if completions.count_ones() >= 2 {
        return Some(MATE - ply as i32 - 3);
    }
    let cell = completions.bits().next()?;
    let (r, c) = (cell / STRIDE, cell % STRIDE);
    b.make(opp, r, c);
    // The defender's block cannot win: `cell` is on board and empty, and their five mask was empty
    // (checked just above), so occupying it cannot complete a five for them.
    let value = walk(b, side, ply + 2, depth - 2, false, budget);
    b.unmake(opp, r, c);
    value
}

#[cfg(test)]
mod tests;
