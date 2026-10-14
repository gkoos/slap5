//! Iterative-deepening principal variation search (negamax) with a transposition table, killer moves and history.
//! A search interrupted by the deadline is discarded: the move comes from the last completed iteration.
//! A forced reply to an open three does not reduce the remaining depth.

use crate::bitboard::{Bits, SIZE, STRIDE};
use crate::{Board, Words, eval, zobrist};
use std::time::{Duration, Instant};

pub const MATE: i32 = 30_000;
const INF: i32 = 32_000;
const MAX_PLY: usize = 256;
/// Scores at or beyond this magnitude are wins or losses.
const MATE_BOUND: i32 = MATE - MAX_PLY as i32;
const NO_MOVE: u8 = 255;
const DEFAULT_TT_BITS: u32 = 21; // 2^21 entries of 16 bytes = 32 MB

const TT_MOVE_SCORE: i32 = 1_000_000;
const KILLER_SCORES: [i32; 2] = [4_000, 3_000];
const HISTORY_DIVISOR: i32 = 32;
/// History values are halved once one passes this.
const HISTORY_CAP: i32 = 100_000;

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
    pub stats: Stats,
    pub elapsed: Duration,
}

/// Switches and limits. The defaults are the engine's settings; tests and ablation runs change them.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Transposition table.
    pub use_tt: bool,
    /// The opponent has an open three: blocks and our own fours are the only moves and cost no depth.
    pub free_three_replies: bool,
    /// Free plies stop at this ply from the root.
    pub free_ply_limit: usize,
    /// Most voluntary fours in the leaf search at depth 0; 0 evaluates at once.
    pub leaf_depth: i32,
    /// Move-count pruning and late move reductions of quiet moves. Off makes the search exact.
    pub selective: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { use_tt: true, free_three_replies: true, free_ply_limit: 64, leaf_depth: 1, selective: true }
    }
}

impl Config {
    /// Applies a command-line switch such as `no-leaf` or `leaf=2`; false if it is not one.
    pub fn apply(&mut self, flag: &str) -> bool {
        match flag {
            "no-tt" => self.use_tt = false,
            "no-threes" => self.free_three_replies = false,
            "no-leaf" => self.leaf_depth = 0,
            "no-selective" => self.selective = false,
            _ => match flag.strip_prefix("leaf=").and_then(|v| v.parse().ok()) {
                Some(n) => self.leaf_depth = n,
                None => return false,
            },
        }
        true
    }
}

/// Counters for one search.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub nodes: u64,
    /// Nodes where the side to move was forced (and so searched without losing depth).
    pub forced_nodes: u64,
    /// Calls of the leaf search for fours (see `Searcher::leaf`).
    pub leaf_nodes: u64,
    /// Nodes at the horizon (depth 0), where the leaf search or the evaluation starts.
    pub horizon_nodes: u64,
    /// Leaf-search calls that played at least one four instead of evaluating.
    pub leaf_expansions: u64,
    /// Beta cutoffs in the main search, and how many came from the first move tried.
    pub cutoffs: u64,
    pub first_move_cutoffs: u64,
    /// Moves skipped by the move-count limit.
    pub pruned: u64,
    /// Interior nodes (not the root) by [forced][tactical][cut][depth, capped at 8].
    pub width: [[[[Width; 9]; 2]; 2]; 2],
}

/// Interior nodes of one kind, the moves they had, and the moves they searched.
#[derive(Clone, Copy, Default, Debug)]
pub struct Width {
    pub nodes: u64,
    pub candidates: u64,
    pub searched: u64,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Stats { nodes, forced_nodes, leaf_nodes, horizon_nodes, leaf_expansions, cutoffs, first_move_cutoffs, pruned, .. } = self;
        write!(
            f,
            "nodes {nodes} forced {forced_nodes} leaf {leaf_nodes} horizon {horizon_nodes} expansions {leaf_expansions} cutoffs {cutoffs} first {first_move_cutoffs} pruned {pruned}"
        )
    }
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
    /// The move played at each ply of the current line.
    path: [u8; MAX_PLY],
    history: [[i32; 256]; 2],
    /// The first iteration: untimed, and without free plies or the leaf search, so it is cheap and always completes.
    plain_iteration: bool,
    stopped: bool,
    deadline: Instant,
    root_move: u8,
    root_hint: u8,
    stats: Stats,
    pub config: Config,
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

fn to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score + ply as i32
    } else if score <= -MATE_BOUND {
        score - ply as i32
    } else {
        score
    }
}

fn from_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_BOUND {
        score - ply as i32
    } else if score <= -MATE_BOUND {
        score + ply as i32
    } else {
        score
    }
}

/// Cells that complete a five or make an open four, for the side to move and the opponent.
struct Threats {
    own_five: Words,
    opp_five: Words,
    own_open4: Words,
    opp_open4: Words,
}

impl Threats {
    fn new(b: &Board, side: usize) -> Self {
        let opp = 1 - side;
        Threats {
            own_five: b.five_cells(side),
            opp_five: b.five_cells(opp),
            own_open4: b.open_four_cells(side),
            opp_open4: b.open_four_cells(opp),
        }
    }

    /// The score of a node decided without search, if it is: a full board, our five next move, two fives
    /// threatened against us (we can block only one), or an open four for us with no five threatened against us.
    fn decided(&self, ply: usize, stones: u32) -> Option<i32> {
        if stones as usize == SIZE * SIZE {
            return Some(0);
        }
        if !self.own_five.is_zero() {
            return Some(MATE - ply as i32 - 1);
        }
        let mut opp_five = self.opp_five;
        if opp_five.pop_lowest().is_some() {
            return (!opp_five.is_zero()).then_some(-(MATE - ply as i32 - 2));
        }
        (!self.own_open4.is_zero()).then_some(MATE - ply as i32 - 3)
    }
}

// Static move scores from the threat masks of both sides: the cells that would make a threat for us
// and the cells that would make the same threat for the opponent (blocking them).
struct MoveMasks {
    opp_five: Words,
    own_open4: Words,
    opp_open4: Words,
    own_four: Words,
    opp_four: Words,
    own_open3: Words,
    opp_open3: Words,
    own_three: Words,
    opp_three: Words,
    own_two: Words,
    opp_two: Words,
}

impl MoveMasks {
    fn new(b: &Board, side: usize, t: &Threats) -> Self {
        let opp = 1 - side;
        let (own_three, own_two) = b.three_and_two_cells(side);
        let (opp_three, opp_two) = b.three_and_two_cells(opp);
        MoveMasks {
            opp_five: t.opp_five,
            own_open4: t.own_open4,
            opp_open4: t.opp_open4,
            own_four: b.four_cells(side),
            opp_four: b.four_cells(opp),
            own_open3: b.open_three_cells(side),
            opp_open3: b.open_three_cells(opp),
            own_three,
            opp_three,
            own_two,
            opp_two,
        }
    }

    /// Score from the fives, fours and open threes a move makes or blocks; zero makes it "quiet".
    fn threat_score(&self, i: usize) -> i32 {
        let weights = [
            (&self.opp_five, 20_000),
            (&self.own_open4, 10_000),
            (&self.opp_open4, 5_000),
            (&self.own_four, 2_000),
            (&self.own_open3, 1_000),
            (&self.opp_four, 800),
            (&self.opp_open3, 500),
        ];
        weights.iter().filter(|(m, _)| m.get(i)).map(|(_, w)| w).sum()
    }

    /// Score from the threes and twos (in a window of five) a move makes or blocks. Untuned weights.
    fn shape_score(&self, i: usize) -> i32 {
        let weights = [(&self.own_three, 250), (&self.opp_three, 200), (&self.own_two, 40), (&self.opp_two, 30)];
        weights.iter().filter(|(m, _)| m.get(i)).map(|(_, w)| w).sum()
    }
}

/// Moves a node searches before it stops: few when a four or open three is on the board.
fn move_count_limit(depth: i32, tactical: bool) -> usize {
    let d = f64::from(depth).sqrt();
    if tactical {
        ((5.0 * d - 2.0).round() as i64).clamp(2, 8) as usize
    } else {
        ((15.0 * d - 12.0).round() as i64).clamp(3, 32) as usize
    }
}

/// Chebyshev distance between two cells.
fn distance(a: usize, b: usize) -> usize {
    (a / STRIDE).abs_diff(b / STRIDE).max((a % STRIDE).abs_diff(b % STRIDE))
}

/// Plies taken off the first search of the `k`-th quiet move.
fn late_move_reduction(depth: i32, k: usize, null_window: bool) -> i32 {
    if depth < 3 || k < 3 {
        return 0;
    }
    1 + (depth >= 6 && k >= 8) as i32 + null_window as i32
}

#[derive(Clone, Copy, Default)]
struct Move {
    score: i32,
    cell: u8,
    /// Makes or blocks no pattern and is not a killer: the only kind pruning and reductions touch.
    quiet: bool,
}

/// Yields moves best first with their position in the order (selection sort, one step per move taken).
struct MoveList {
    moves: [Move; SIZE * SIZE],
    len: usize,
    next: usize,
}

impl Iterator for MoveList {
    type Item = (usize, Move);

    fn next(&mut self) -> Option<(usize, Move)> {
        let k = self.next;
        if k == self.len {
            return None;
        }
        let mut pick = k;
        for j in k + 1..self.len {
            if self.moves[j].score > self.moves[pick].score {
                pick = j;
            }
        }
        self.moves.swap(k, pick);
        self.next += 1;
        Some((k, self.moves[k]))
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
            path: [NO_MOVE; MAX_PLY],
            history: [[0; 256]; 2],
            plain_iteration: true,
            stopped: false,
            deadline: Instant::now(),
            root_move: NO_MOVE,
            root_hint: NO_MOVE,
            stats: Stats::default(),
            config: Config::default(),
        }
    }

    /// Best move for `side` to move, or None if the board is full.
    pub fn search(&mut self, board: &Board, side: usize, limits: Limits) -> Option<SearchResult> {
        let start = Instant::now();
        let stones = board.stone_count();
        if stones as usize == SIZE * SIZE {
            return None;
        }
        let found = |idx: usize, score: i32, depth: i32, stats: Stats| SearchResult {
            x: idx % STRIDE,
            y: idx / STRIDE,
            score,
            depth,
            stats,
            elapsed: start.elapsed(),
        };
        if stones == 0 {
            return Some(found((SIZE / 2) * STRIDE + SIZE / 2, 0, 0, Stats::default()));
        }
        if let Some(i) = board.five_cells(side).pop_lowest() {
            return Some(found(i, MATE - 1, 1, Stats::default()));
        }

        self.age = self.age.wrapping_add(1);
        self.stats = Stats::default();
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
            self.plain_iteration = depth == 1;
            let score = self.node(&mut b, side, depth, -INF, INF, 0, stones);
            if self.stopped {
                break;
            }
            best = Some((self.root_move, score, depth));
            self.root_hint = self.root_move;
            if score.abs() >= MATE_BOUND || start.elapsed() * 2 > limits.time {
                break;
            }
        }
        best.map(|(m, s, d)| found(m as usize, s, d, self.stats))
    }

    /// Counts a node and checks the deadline every 1024 nodes; true once the search must stop.
    fn tick(&mut self) -> bool {
        self.stats.nodes += 1;
        if !self.plain_iteration && self.stats.nodes & 1023 == 0 && Instant::now() >= self.deadline {
            self.stopped = true;
        }
        self.stopped
    }

    /// A stored score that settles the node, and the stored best move (`NO_MOVE` if none).
    fn probe_tt(&self, key: u64, depth: i32, alpha: i32, beta: i32, ply: usize) -> (Option<i32>, u8) {
        let e = self.tt[key as usize & self.mask];
        if !self.config.use_tt || e.bound == 0 || e.key != key {
            return (None, NO_MOVE);
        }
        let s = from_tt(e.score as i32, ply);
        let settles = e.depth as i32 >= depth
            && match e.bound {
                EXACT => true,
                LOWER => s >= beta,
                UPPER => s <= alpha,
                _ => false,
            };
        (settles.then_some(s), e.mv)
    }

    #[allow(clippy::too_many_arguments)]
    fn store_tt(&mut self, key: u64, depth: i32, score: i32, alpha: i32, beta: i32, mv: u8, ply: usize) {
        if !self.config.use_tt {
            return;
        }
        let bound = if score <= alpha {
            UPPER
        } else if score >= beta {
            LOWER
        } else {
            EXACT
        };
        let age = self.age;
        let slot = &mut self.tt[key as usize & self.mask];
        if slot.bound != 0 && slot.age == age && (depth as i8) < slot.depth {
            return;
        }
        let mv = if mv == NO_MOVE && slot.key == key { slot.mv } else { mv };
        *slot = Entry { key, score: to_tt(score, ply) as i16, mv, depth: depth as i8, bound, age };
    }

    fn order_moves(&self, masks: &MoveMasks, mut cand: Words, side: usize, tt_move: u8, ply: usize) -> MoveList {
        let killers = self.killers[ply];
        let recent = &self.path[ply.saturating_sub(4)..ply];
        let mut list = MoveList { moves: [Move::default(); SIZE * SIZE], len: 0, next: 0 };
        while let Some(i) = cand.pop_lowest() {
            let cell = i as u8;
            list.moves[list.len] = if cell == tt_move {
                Move { score: TT_MOVE_SCORE, cell, quiet: false }
            } else {
                let threat = masks.threat_score(i);
                // Moves more than 4 cells from the last two (or four) moves played lose half (three quarters).
                let far = |moves: &[u8]| !moves.is_empty() && moves.iter().all(|&m| distance(i, m as usize) > 4);
                let shape = masks.shape_score(i);
                let shape = if far(recent) {
                    shape / 4
                } else if far(&recent[recent.len().saturating_sub(2)..]) {
                    shape / 2
                } else {
                    shape
                };
                let killer = killers.iter().position(|&k| k == cell).map_or(0, |p| KILLER_SCORES[p]);
                let score = threat + shape + killer + self.history[side][i] / HISTORY_DIVISOR;
                Move { score, cell, quiet: threat == 0 && killer == 0 }
            };
            list.len += 1;
        }
        list
    }

    fn on_cutoff(&mut self, side: usize, ply: usize, cell: u8, depth: i32, k: usize) {
        self.stats.cutoffs += 1;
        self.stats.first_move_cutoffs += (k == 0) as u64;
        let killers = &mut self.killers[ply];
        if killers[0] != cell {
            killers[1] = killers[0];
            killers[0] = cell;
        }
        let history = &mut self.history[side];
        history[cell as usize] += depth * depth;
        if history[cell as usize] > HISTORY_CAP {
            for h in history.iter_mut() {
                *h /= 2;
            }
        }
    }

    /// A node inside the leaf search: counted, its threats built and checked, then searched by `leaf`.
    #[allow(clippy::too_many_arguments)]
    fn leaf_child(&mut self, b: &mut Board, side: usize, alpha: i32, beta: i32, ply: usize, stones: u32, qdepth: i32) -> i32 {
        if self.tick() {
            return 0;
        }
        let t = Threats::new(b, side);
        if let Some(s) = t.decided(ply, stones) {
            return s;
        }
        self.leaf(b, side, &t, alpha, beta, ply, stones, qdepth)
    }

    /// Search at depth 0: the side to move may play fours, each forcing one reply, instead of
    /// evaluating at once (or keep the static evaluation, "stand pat"). `qdepth` limits the fours.
    /// The caller has counted the node and checked `Threats::decided`.
    #[allow(clippy::too_many_arguments)]
    fn leaf(&mut self, b: &mut Board, side: usize, t: &Threats, mut alpha: i32, beta: i32, ply: usize, stones: u32, qdepth: i32) -> i32 {
        self.stats.leaf_nodes += 1;
        let opp = 1 - side;
        let mut opp_five = t.opp_five;
        if let Some(c) = opp_five.pop_lowest() {
            let (r, col) = (c / STRIDE, c % STRIDE);
            b.make(side, r, col);
            let s = -self.leaf_child(b, opp, -beta, -alpha, ply + 1, stones + 1, qdepth);
            b.unmake(side, r, col);
            return s;
        }
        let stand = eval::evaluate(b, side);
        if qdepth <= 0 || stand >= beta {
            return stand;
        }
        let mut best = stand;
        alpha = alpha.max(stand);
        // No open-four cell here: `decided` would have scored it as a win.
        let mut cand = b.four_cells(side);
        if !cand.is_zero() {
            self.stats.leaf_expansions += 1;
        }
        while let Some(i) = cand.pop_lowest() {
            let (r, c) = (i / STRIDE, i % STRIDE);
            b.make(side, r, c);
            let s = -self.leaf_child(b, opp, -beta, -alpha, ply + 1, stones + 1, qdepth - 1);
            b.unmake(side, r, c);
            if self.stopped {
                return 0;
            }
            if s > best {
                best = s;
                alpha = alpha.max(s);
                if alpha >= beta {
                    return best;
                }
            }
        }
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn node(&mut self, b: &mut Board, side: usize, depth: i32, mut alpha: i32, beta: i32, ply: usize, stones: u32) -> i32 {
        if self.tick() {
            return 0;
        }
        let root = ply == 0;
        let t = Threats::new(b, side);
        if !root && let Some(s) = t.decided(ply, stones) {
            return s;
        }

        // A four against us leaves only its block. Against an open three only its blocks and our own fours
        // are tried, and (with `free_three_replies`) they cost no depth.
        let opp = 1 - side;
        let block_four = !t.opp_five.is_zero();
        let three_reply = self.config.free_three_replies && !self.plain_iteration && !block_four && !t.opp_open4.is_zero();
        let forced = block_four || three_reply;
        let free = three_reply && ply < self.config.free_ply_limit;
        if !root && depth <= 0 && !free {
            self.stats.horizon_nodes += 1;
            return if self.config.leaf_depth > 0 && !self.plain_iteration {
                self.leaf(b, side, &t, alpha, beta, ply, stones, self.config.leaf_depth)
            } else {
                eval::evaluate(b, side)
            };
        }
        if forced {
            self.stats.forced_nodes += 1;
        }
        let child_depth = if free { depth } else { depth - 1 };

        let key = b.hash() ^ if side == 1 { zobrist::SIDE } else { 0 };
        let tt_move = if root {
            self.root_hint
        } else {
            let (settled, mv) = self.probe_tt(key, depth, alpha, beta, ply);
            if let Some(s) = settled {
                return s;
            }
            mv
        };

        let masks = MoveMasks::new(b, side, &t);
        let cand = if block_four {
            t.opp_five
        } else if three_reply {
            b.open_four_block_cells(opp).or(masks.own_four)
        } else {
            b.candidates()
        };
        let moves = self.order_moves(&masks, cand, side, tt_move, ply);
        if moves.len == 0 {
            return 0;
        }

        let selective = self.config.selective && !root && !forced;
        let null_window = beta - alpha == 1;
        // "Tactical": either side already has a four or an open three on the board.
        let tactical = !(t.opp_five.is_zero() && t.opp_open4.is_zero() && t.own_open4.is_zero());
        let limit = move_count_limit(depth, tactical);
        let candidates = moves.len as u64;
        let mut searched = 0;
        let mut cut = false;
        let orig_alpha = alpha;
        let mut best = -INF;
        let mut best_move = NO_MOVE;
        for (k, m) in moves {
            // Past the limit every move is skipped; one ply from the root only quiet ones are.
            // Nothing is skipped while every move so far loses: one of the skipped moves might not.
            if selective && k >= limit && best > -MATE_BOUND && (ply >= 2 || m.quiet) {
                self.stats.pruned += 1;
                continue;
            }
            let reduction = if selective && m.quiet { late_move_reduction(depth, k, null_window) } else { 0 };
            searched += 1;
            self.path[ply] = m.cell;
            let (r, c) = (m.cell as usize / STRIDE, m.cell as usize % STRIDE);
            b.make(side, r, c);
            // After the first move a null window only asks whether the move beats alpha.
            let window = if k == 0 { beta } else { alpha + 1 };
            let mut score = -self.node(b, opp, child_depth - reduction, -window, -alpha, ply + 1, stones + 1);
            if reduction > 0 && score > alpha {
                score = -self.node(b, opp, child_depth, -window, -alpha, ply + 1, stones + 1);
            }
            if window < beta && score > alpha && score < beta {
                score = -self.node(b, opp, child_depth, -beta, -alpha, ply + 1, stones + 1);
            }
            b.unmake(side, r, c);
            if self.stopped {
                return 0;
            }
            if score > best {
                best = score;
                best_move = m.cell;
                if root {
                    self.root_move = m.cell;
                }
                if score > alpha {
                    alpha = score;
                    if alpha >= beta {
                        self.on_cutoff(side, ply, m.cell, depth, k);
                        cut = true;
                        break;
                    }
                }
            }
        }

        if !root {
            let w = &mut self.stats.width[forced as usize][tactical as usize][cut as usize][depth.clamp(0, 8) as usize];
            w.nodes += 1;
            w.candidates += candidates;
            w.searched += searched;
        }
        self.store_tt(key, depth, best, orig_alpha, beta, best_move, ply);
        best
    }
}

#[cfg(test)]
mod tests;
