//! Iterative-deepening alpha-beta (negamax) with a transposition table, killer moves and history.
//! A search interrupted by the deadline is discarded: the move comes from the last completed iteration.

use crate::bitboard::{Bits, SIZE, STRIDE};
use crate::{Board, Words, eval, zobrist};
use std::time::{Duration, Instant};

pub const MATE: i32 = 30_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 128;
const NO_MOVE: u8 = 255;
const DEFAULT_TT_BITS: u32 = 21; // 2^21 entries of 16 bytes = 32 MB

const LOWER: u8 = 1;
const UPPER: u8 = 2;
const EXACT: u8 = 3;

#[derive(Clone, Copy)]
pub struct Limits {
    pub time: Duration,
    pub max_depth: i32,
}

pub struct SearchResult {
    pub x: usize,
    pub y: usize,
    pub score: i32,
    /// Depth of the last completed iteration.
    pub depth: i32,
    pub nodes: u64,
    pub elapsed: Duration,
}

#[derive(Clone, Copy, Default)]
struct Entry {
    key: u64,
    score: i16,
    mv: u8,
    depth: i8,
    bound: u8,
    age: u8,
}

pub struct Searcher {
    tt: Vec<Entry>,
    mask: usize,
    age: u8,
    killers: [[u8; 2]; MAX_PLY],
    history: [[i32; 256]; 2],
    nodes: u64,
    stopped: bool,
    timed: bool,
    deadline: Instant,
    root_move: u8,
    root_hint: u8,
    /// Switch the transposition table off (for tests and ablation).
    pub use_tt: bool,
    /// Switch move ordering off: moves are searched in cell order (for tests and ablation).
    pub ordered: bool,
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

fn to_tt(score: i32, ply: usize) -> i32 {
    let limit = MATE - MAX_PLY as i32;
    if score >= limit {
        score + ply as i32
    } else if score <= -limit {
        score - ply as i32
    } else {
        score
    }
}

fn from_tt(score: i32, ply: usize) -> i32 {
    let limit = MATE - MAX_PLY as i32;
    if score >= limit {
        score - ply as i32
    } else if score <= -limit {
        score + ply as i32
    } else {
        score
    }
}

impl Searcher {
    pub fn new() -> Self {
        Self::with_table_bits(DEFAULT_TT_BITS)
    }

    pub fn with_table_bits(bits: u32) -> Self {
        Searcher {
            tt: vec![Entry::default(); 1 << bits],
            mask: (1 << bits) - 1,
            age: 0,
            killers: [[NO_MOVE; 2]; MAX_PLY],
            history: [[0; 256]; 2],
            nodes: 0,
            stopped: false,
            timed: false,
            deadline: Instant::now(),
            root_move: NO_MOVE,
            root_hint: NO_MOVE,
            use_tt: true,
            ordered: true,
        }
    }

    /// Best move for `side` to move, or None if the board is full.
    pub fn search(&mut self, board: &Board, side: usize, limits: Limits) -> Option<SearchResult> {
        let start = Instant::now();
        let stones = board.stone_count();
        if stones as usize == SIZE * SIZE {
            return None;
        }
        let found = |idx: usize, score: i32, depth: i32, nodes: u64| SearchResult {
            x: idx % STRIDE,
            y: idx / STRIDE,
            score,
            depth,
            nodes,
            elapsed: start.elapsed(),
        };
        if stones == 0 {
            return Some(found((SIZE / 2) * STRIDE + SIZE / 2, 0, 0, 0));
        }
        if let Some(i) = board.five_cells(side).pop_lowest() {
            return Some(found(i, MATE - 1, 1, 0));
        }

        self.age = self.age.wrapping_add(1);
        self.nodes = 0;
        self.stopped = false;
        self.deadline = start + limits.time;
        self.killers = [[NO_MOVE; 2]; MAX_PLY];
        for h in self.history.iter_mut().flatten() {
            *h /= 2;
        }
        self.root_hint = NO_MOVE;

        let mut b = *board;
        let mut best = None;
        for depth in 1..=limits.max_depth {
            // Depth 1 always completes so that there is a move to return.
            self.timed = depth > 1;
            let score = self.node(&mut b, side, depth, -INF, INF, 0, stones);
            if self.stopped {
                break;
            }
            best = Some((self.root_move, score, depth));
            self.root_hint = self.root_move;
            if score.abs() >= MATE - MAX_PLY as i32 || start.elapsed() * 2 > limits.time {
                break;
            }
        }
        best.map(|(m, s, d)| found(m as usize, s, d, self.nodes))
    }

    fn probe(&self, key: u64) -> Option<Entry> {
        let e = self.tt[key as usize & self.mask];
        (e.bound != 0 && e.key == key).then_some(e)
    }

    fn store(&mut self, key: u64, depth: i32, score: i32, bound: u8, mv: u8) {
        let age = self.age;
        let slot = &mut self.tt[key as usize & self.mask];
        if slot.bound != 0 && slot.age == age && (depth as i8) < slot.depth {
            return;
        }
        let mv = if mv == NO_MOVE && slot.key == key { slot.mv } else { mv };
        *slot = Entry { key, score: score as i16, mv, depth: depth as i8, bound, age };
    }

    #[allow(clippy::too_many_arguments)]
    fn node(&mut self, b: &mut Board, side: usize, depth: i32, mut alpha: i32, beta: i32, ply: usize, stones: u32) -> i32 {
        self.nodes += 1;
        if self.timed && self.nodes & 1023 == 0 && Instant::now() >= self.deadline {
            self.stopped = true;
        }
        if self.stopped {
            return 0;
        }
        let root = ply == 0;
        let opp = 1 - side;
        if !root {
            if stones as usize == SIZE * SIZE {
                return 0;
            }
            if !b.five_cells(side).is_zero() {
                return MATE - ply as i32 - 1;
            }
            if depth <= 0 {
                return eval::evaluate(b, side);
            }
        }

        let key = b.hash() ^ if side == 1 { zobrist::SIDE } else { 0 };
        let mut tt_move = if root { self.root_hint } else { NO_MOVE };
        if self.use_tt
            && let Some(e) = self.probe(key)
            && !root
        {
            tt_move = e.mv;
            if e.depth as i32 >= depth {
                let s = from_tt(e.score as i32, ply);
                match e.bound {
                    EXACT => return s,
                    LOWER if s >= beta => return s,
                    UPPER if s <= alpha => return s,
                    _ => {}
                }
            }
        }

        let mut list = [(0i32, 0u8); SIZE * SIZE];
        let mut n = 0;
        let (block, own_open4, opp_open4) = if self.ordered {
            (b.five_cells(opp), b.open_four_cells(side), b.open_four_cells(opp))
        } else {
            (Words::ZERO, Words::ZERO, Words::ZERO)
        };
        let killers = self.killers[ply];
        let mut cand = b.candidates();
        while let Some(i) = cand.pop_lowest() {
            let m = i as u8;
            let s = if !self.ordered {
                0
            } else if m == tt_move {
                1_000_000
            } else if block.get(i) {
                900_000
            } else if own_open4.get(i) {
                800_000
            } else if opp_open4.get(i) {
                700_000
            } else if m == killers[0] {
                500_000
            } else if m == killers[1] {
                490_000
            } else {
                self.history[side][i]
            };
            list[n] = (s, m);
            n += 1;
        }
        if n == 0 {
            return 0;
        }

        let orig_alpha = alpha;
        let mut best = -INF;
        let mut best_move = NO_MOVE;
        for k in 0..n {
            let mut pick = k;
            for j in k + 1..n {
                if list[j].0 > list[pick].0 {
                    pick = j;
                }
            }
            list.swap(k, pick);
            let mv = list[k].1;
            let (r, c) = (mv as usize / STRIDE, mv as usize % STRIDE);
            b.make(side, r, c);
            let score = -self.node(b, opp, depth - 1, -beta, -alpha, ply + 1, stones + 1);
            b.unmake(side, r, c);
            if self.stopped {
                return 0;
            }
            if score > best {
                best = score;
                best_move = mv;
                if root {
                    self.root_move = mv;
                }
                if score > alpha {
                    alpha = score;
                    if alpha >= beta {
                        if self.killers[ply][0] != mv {
                            self.killers[ply][1] = self.killers[ply][0];
                            self.killers[ply][0] = mv;
                        }
                        let h = &mut self.history[side][mv as usize];
                        *h += depth * depth;
                        if *h > 400_000 {
                            for h in self.history[side].iter_mut() {
                                *h /= 2;
                            }
                        }
                        break;
                    }
                }
            }
        }

        if self.use_tt {
            let bound = if best <= orig_alpha {
                UPPER
            } else if best >= beta {
                LOWER
            } else {
                EXACT
            };
            self.store(key, depth, to_tt(best, ply), bound, best_move);
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    fn board(stones: &[(usize, usize, usize)]) -> Board {
        let mut b = Board::new();
        for &(p, r, c) in stones {
            b.make(p, r, c);
        }
        b
    }

    fn limits(depth: i32) -> Limits {
        Limits { time: HOUR, max_depth: depth }
    }

    fn searcher() -> Searcher {
        Searcher::with_table_bits(16)
    }

    // Same terminal rules as `Searcher::node`, but no pruning, ordering or table.
    fn minimax(b: &mut Board, side: usize, depth: i32, ply: usize, stones: u32) -> i32 {
        if stones as usize == SIZE * SIZE {
            return 0;
        }
        if !b.five_cells(side).is_zero() {
            return MATE - ply as i32 - 1;
        }
        if depth <= 0 {
            return eval::evaluate(b, side);
        }
        let mut best = -INF;
        let mut cand = b.candidates();
        while let Some(i) = cand.pop_lowest() {
            let (r, c) = (i / STRIDE, i % STRIDE);
            b.make(side, r, c);
            best = best.max(-minimax(b, 1 - side, depth - 1, ply + 1, stones + 1));
            b.unmake(side, r, c);
        }
        best
    }

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 3) as usize
        }
    }

    // `n` stones alternating between the players inside a `size` x `size` window.
    fn random_position(rng: &mut Rng, n: usize, size: usize) -> Board {
        let (r0, c0) = (3 + rng.next() % 6, 3 + rng.next() % 6);
        let mut b = Board::new();
        let mut placed = Vec::new();
        while placed.len() < n {
            let cell = (r0 + rng.next() % size, c0 + rng.next() % size);
            if !placed.contains(&cell) {
                b.make(placed.len() % 2, cell.0, cell.1);
                placed.push(cell);
            }
        }
        b
    }

    fn agrees_with_minimax(depth: i32, positions: usize, stones: usize, window: usize, ordered: bool) {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for _ in 0..positions {
            let b = random_position(&mut rng, stones, window);
            let mut s = searcher();
            s.use_tt = false;
            s.ordered = ordered;
            let r = s.search(&b, 0, limits(depth)).unwrap();
            let mut copy = b;
            let expected = minimax(&mut copy, 0, r.depth, 0, b.stone_count());
            assert_eq!(r.score, expected, "depth {} ordered {ordered}", r.depth);
        }
    }

    #[test]
    fn matches_minimax_at_depth_two() {
        // Slow in debug builds; a release run covers many more positions.
        let positions = if cfg!(debug_assertions) { 12 } else { 300 };
        agrees_with_minimax(2, positions, 7, 4, true);
        agrees_with_minimax(2, positions, 7, 4, false);
    }

    #[test]
    #[ignore = "slow in debug builds; run with --release -- --ignored"]
    fn matches_minimax_at_depth_three() {
        agrees_with_minimax(3, 4, 6, 3, true);
        agrees_with_minimax(3, 4, 6, 3, false);
    }

    #[test]
    fn empty_board_plays_the_centre() {
        let r = searcher().search(&Board::new(), 0, limits(4)).unwrap();
        assert_eq!((r.x, r.y), (7, 7));
    }

    #[test]
    fn full_board_has_no_move() {
        let mut b = Board::new();
        for r in 0..SIZE {
            for c in 0..SIZE {
                b.make((r + c) % 2, r, c);
            }
        }
        assert!(searcher().search(&b, 0, limits(2)).is_none());
    }

    #[test]
    fn takes_an_immediate_win() {
        let b = board(&[(0, 7, 3), (0, 7, 4), (0, 7, 5), (0, 7, 6), (1, 10, 10), (1, 11, 10)]);
        let r = searcher().search(&b, 0, limits(4)).unwrap();
        assert_eq!(r.score, MATE - 1);
        assert_eq!(r.y, 7);
        assert!(r.x == 2 || r.x == 7);
    }

    #[test]
    fn blocks_a_blocked_four() {
        // O has 3..6 on row 7 with X on (7,2): only (7,7) completes five.
        let b = board(&[(1, 7, 3), (1, 7, 4), (1, 7, 5), (1, 7, 6), (0, 7, 2), (0, 12, 12)]);
        let r = searcher().search(&b, 0, limits(3)).unwrap();
        assert_eq!((r.x, r.y), (7, 7));
        assert!(r.score.abs() < MATE - 200);
    }

    fn open_three_win(use_tt: bool, ordered: bool) {
        let b = board(&[(0, 7, 5), (0, 7, 6), (0, 7, 7), (1, 0, 0), (1, 0, 14)]);
        let mut s = searcher();
        s.use_tt = use_tt;
        s.ordered = ordered;
        let r = s.search(&b, 0, limits(5)).unwrap();
        assert_eq!(r.score, MATE - 3, "tt {use_tt} ordered {ordered}");
        assert_eq!(r.y, 7);
        assert!(r.x == 4 || r.x == 8, "x = {}", r.x);
    }

    #[test]
    fn wins_with_an_open_four() {
        open_three_win(true, true);
        open_three_win(false, true);
        open_three_win(true, false);
        open_three_win(false, false);
    }

    #[test]
    fn deadline_is_respected_and_the_aborted_iteration_is_discarded() {
        let mut rng = Rng(0xABCD_EF01_2345_6789);
        let b = loop {
            let b = random_position(&mut rng, 18, 7);
            if b.five_cells(0).is_zero() && b.five_cells(1).is_zero() && !b.wins(0) && !b.wins(1) {
                break b;
            }
        };
        let started = Instant::now();
        let timed = searcher().search(&b, 0, Limits { time: Duration::from_millis(150), max_depth: 100 }).unwrap();
        assert!(started.elapsed() < Duration::from_millis(1000), "took {:?}", started.elapsed());
        assert!(timed.depth >= 1 && timed.depth < 100);

        let fixed = searcher().search(&b, 0, limits(timed.depth)).unwrap();
        assert_eq!((timed.x, timed.y, timed.score), (fixed.x, fixed.y, fixed.score));
    }
}
