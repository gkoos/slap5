//! Iterative-deepening principal variation search (negamax) with a transposition table, killer moves and history.
//! A search interrupted by the deadline is discarded: the move comes from the last completed iteration.
//! A forced reply to an open three does not reduce the remaining depth.

use crate::bitboard::{Bits, SIZE, STRIDE};
use crate::threats::{self, SHAPES, Shapes};
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
    /// Whether a reported mate was checked with the table off over its own mate distance. `None` when no
    /// mate was reported, `Some(false)` when the mate was too far away to check inside the turn, which
    /// means it is reported and not proven (see `docs/engine-audit-followup.md`, finding 4).
    pub mate_verified: Option<bool>,
    pub stats: Stats,
    pub elapsed: Duration,
}

/// Switches and limits. The defaults are the engine's settings; tests and ablation runs change them.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Transposition table.
    pub use_tt: bool,
    /// The opponent has an open three: blocks and our own fours are the only moves.
    pub free_three_replies: bool,
    /// Those replies cost no depth. `no-three-ext` turns only this off: the narrowed reply set stays and
    /// the reply is charged a ply. The set branches and a reply may itself be a threat, so a free ply
    /// here can let one subtree take the whole turn (see `docs/engine-audit-followup.md`). Default on.
    pub extend_three_replies: bool,
    /// The opponent threatens five: the block is its only reply and costs no depth, so the main
    /// search itself follows four chains. This is the forcing play the depth-0 search used to repeat.
    pub free_four_replies: bool,
    /// The answer to an open three may be a counter-threat of ours, an open three of our own, and not
    /// only a block. Off by default: the wider set scored 19/21 against 20/21 on the suite.
    pub counter_threes: bool,
    /// Refuse to store a fail-low from a node that skipped moves. Off by default: the strict version
    /// scored 4W–16L against Tito where storing it scored 7W–13L, at a cost of one suite position.
    pub strict_bounds: bool,
    /// A move that makes two threats at once (a double four, or a four and an open three) is searched
    /// one ply deeper: it is the shape that usually decides the game.
    pub double_threat_ext: bool,
    /// Free plies (forced replies) stop at this ply from the root, for threes and fours alike.
    pub free_ply_limit: usize,
    /// A mate no further than this many plies is re-searched with the pruning, the move-count limit and
    /// the table off, which proves it. Further mates cannot be checked inside a turn and are reported
    /// unverified instead (switch `verify=N`).
    pub verify_ply_limit: i32,
    /// How long that check may take (switch `verify-ms=N`). An exact search with the pruning, the limit and
    /// the table off is expensive on a busy board, so the check is bounded and a mate it cannot finish is
    /// reported unverified rather than being trusted.
    pub verify_ms: u64,
    /// Move-count pruning and late move reductions of quiet moves. Off makes the search exact.
    pub selective: bool,
    /// Move-count pruning of quiet moves past the node's limit. Only meaningful with `selective`.
    pub limit: bool,
    /// Late move reductions of quiet moves. Only meaningful with `selective`.
    pub reductions: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            use_tt: true,
            free_three_replies: true,
            extend_three_replies: true,
            free_four_replies: true,
            counter_threes: false,
            strict_bounds: false,
            double_threat_ext: true,
            free_ply_limit: 64,
            verify_ply_limit: 7,
            verify_ms: 50,
            selective: true,
            limit: true,
            reductions: true,
        }
    }
}

impl Config {
    /// Applies a command-line switch such as `no-threes`, `no-fours` or `cap=16`; false if it is not one.
    pub fn apply(&mut self, flag: &str) -> bool {
        if let Some(n) = flag.strip_prefix("cap=") {
            match n.parse::<usize>() {
                Ok(v) => {
                    self.free_ply_limit = v;
                    return true;
                }
                Err(_) => return false,
            }
        }
        if let Some(n) = flag.strip_prefix("verify-ms=") {
            match n.parse::<u64>() {
                Ok(v) => {
                    self.verify_ms = v;
                    return true;
                }
                Err(_) => return false,
            }
        }
        if let Some(n) = flag.strip_prefix("verify=") {
            match n.parse::<i32>() {
                Ok(v) => {
                    self.verify_ply_limit = v;
                    return true;
                }
                Err(_) => return false,
            }
        }
        match flag {
            "no-tt" => self.use_tt = false,
            "no-threes" => self.free_three_replies = false,
            "no-three-ext" => self.extend_three_replies = false,
            "no-fours" => self.free_four_replies = false,
            "no-counters" => self.counter_threes = false,
            "strict-bounds" => self.strict_bounds = true,
            "no-double" => self.double_threat_ext = false,
            "no-selective" => self.selective = false,
            "no-limit" => self.limit = false,
            "no-reductions" => self.reductions = false,
            _ => return false,
        }
        true
    }
}

/// Counters for one search.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub nodes: u64,
    /// The deepest ply from the root any node reached, free replies included. The iteration `depth`
    /// does not count free replies, so this is the figure that can be compared with another engine's
    /// reported depth; the two differ by however long the forcing chains ran.
    pub max_ply: usize,
    /// Nodes spent inside mate verification searches (see `verify_mate`).
    pub verify_nodes: u64,
    /// Nodes where the side to move was forced (and so searched without losing depth).
    pub forced_nodes: u64,
    /// Nodes at the horizon (depth 0), where the evaluation starts.
    pub horizon_nodes: u64,
    /// Fail-lows that were not stored as bounds because the node skipped moves (see `store_tt`).
    pub skipped_bounds: u64,
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
        let Stats { nodes, max_ply, verify_nodes, forced_nodes, horizon_nodes, cutoffs, first_move_cutoffs, pruned, .. } = self;
        write!(
            f,
            "nodes {nodes} maxply {max_ply} verify {verify_nodes} forced {forced_nodes} horizon {horizon_nodes} cutoffs {cutoffs} first {first_move_cutoffs} pruned {pruned}"
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
    fn new(shapes: &Shapes<Words>, side: usize) -> Self {
        let opp = 1 - side;
        Threats {
            own_five: shapes.cells(side, threats::FIVE),
            opp_five: shapes.cells(opp, threats::FIVE),
            own_open4: shapes.cells(side, threats::OPEN_FOUR),
            opp_open4: shapes.cells(opp, threats::OPEN_FOUR),
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

/// Move scores for one node, from the shape classes of both players. Each class is counted per
/// direction, so a stone that makes two threats scores twice, and a direction contributes only the
/// strongest shape a cell makes in it.
struct Scores {
    /// Shapes that force a reply (five, open four, four, open three) for either side. A move with
    /// no score here is "quiet" and is the only kind pruning and reductions touch.
    strong: [i32; 256],
    /// Blocked three and two: development value, used for ordering but not for the quiet test.
    shape: [i32; 256],
}

/// Ordering score of making a shape in one direction, by class, for us and for the opponent (for
/// the opponent, the score is for blocking it). Untuned.
const ORDER_SCORE: [[i32; SHAPES]; 2] = [
    [20_000, 10_000, 2_000, 1_000, 250, 40],
    [20_000, 5_000, 800, 500, 200, 30],
];

impl Scores {
    fn new(shapes: &Shapes<Words>, side: usize, cand: Words) -> Self {
        let mut strong = [0i32; 256];
        let mut shape = [0i32; 256];
        for (i, weights) in ORDER_SCORE.iter().enumerate() {
            let player = if i == 0 { side } else { 1 - side };
            for dir in shapes.ranked(player) {
                for (class, &weight) in weights.iter().enumerate() {
                    let goal = if class <= threats::OPEN_THREE { &mut strong } else { &mut shape };
                    let mut m = dir[class].and(cand);
                    while let Some(cell) = m.pop_lowest() {
                        goal[cell] += weight;
                    }
                }
            }
        }
        Scores { strong, shape }
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

/// What `Searcher::verify_mate` did with a reported mate.
enum Verdict {
    /// The check proved the mate, so the result stands.
    Agreed,
    /// The check disagreed: this move and score replace the reported mate, and they are a proof too,
    /// because that search ran without pruning, without the move-count limit and without the table.
    Replaced(u8, i32),
    /// The deadline cut the check short.
    Incomplete,
    /// The mate is further away than `verify_ply_limit`, where an exact search cannot finish inside a turn,
    /// so it is reported without proof.
    TooLong,
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
        let found = |idx: usize, score: i32, depth: i32, stats: Stats, mate_verified: Option<bool>| SearchResult {
            x: idx % STRIDE,
            y: idx / STRIDE,
            score,
            depth,
            mate_verified,
            stats,
            elapsed: start.elapsed(),
        };
        if stones == 0 {
            return Some(found((SIZE / 2) * STRIDE + SIZE / 2, 0, 0, Stats::default(), None));
        }
        if let Some(i) = board.five_cells(side).pop_lowest() {
            // A five is available now, which is a proof by construction rather than a search result.
            return Some(found(i, MATE - 1, 1, Stats::default(), Some(true)));
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
        let mut mate_verified = None;
        for depth in 1..=limits.max_depth {
            self.plain_iteration = depth == 1;
            let score = self.node(&mut b, side, depth, -INF, INF, 0, stones);
            if self.stopped {
                break;
            }
            best = Some((self.root_move, score, depth));
            self.root_hint = self.root_move;
            if score.abs() >= MATE_BOUND {
                // A mate stops the iteration loop, so it has to be true. `verify_mate` proves the short
                // ones; a mate too far away to check is reported as unverified rather than trusted.
                match self.verify_mate(board, side, depth, score, stones) {
                    Verdict::Agreed => mate_verified = Some(true),
                    Verdict::Replaced(mv, verified) => {
                        best = Some((mv, verified, depth));
                        self.root_hint = mv;
                        mate_verified = Some(true);
                    }
                    Verdict::Incomplete | Verdict::TooLong => mate_verified = Some(false),
                }
                break;
            }
            if start.elapsed() * 2 > limits.time {
                break;
            }
        }
        best.map(|(m, s, d)| found(m as usize, s, d, self.stats, mate_verified))
    }

    /// Checks a mate the selective search reported, searching its own mate distance (a mate reported at
    /// depth 9 may be a mate in 3, and an exact search of 9 plies would not finish). The move-count limit,
    /// the reductions **and the transposition table** are off: a table the selective search filled answers
    /// this search with the bounds it exists to check — the follow-up audit measured 119 of 120 verification
    /// nodes coming back as table cutoffs, and with the table on the check cannot fail on the position it
    /// was written for.
    fn verify_mate(&mut self, board: &Board, side: usize, depth: i32, score: i32, stones: u32) -> Verdict {
        let need = (MATE - score.abs()).clamp(1, depth);
        if need > self.config.verify_ply_limit {
            return Verdict::TooLong;
        }
        let saved = self.config;
        let saved_deadline = self.deadline;
        self.config.selective = false;
        self.config.use_tt = false;
        // This search has no move limit, so it has to be bounded in time or it would never finish on a
        // busy board; a mate it cannot finish checking is reported unverified.
        self.plain_iteration = false;
        let left = self.deadline.saturating_duration_since(Instant::now());
        let slice = Duration::from_millis(self.config.verify_ms).min(left / 4);
        self.deadline = Instant::now() + slice;
        let before = self.stats.nodes;
        let mut b = *board;
        let verified = self.node(&mut b, side, need, -INF, INF, 0, stones);
        self.stats.verify_nodes += self.stats.nodes - before;
        self.deadline = saved_deadline;
        self.config = saved;
        if self.stopped {
            return Verdict::Incomplete;
        }
        if verified.abs() >= MATE_BOUND && verified.signum() == score.signum() {
            return Verdict::Agreed;
        }
        Verdict::Replaced(self.root_move, verified)
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
    fn store_tt(&mut self, key: u64, depth: i32, score: i32, alpha: i32, beta: i32, mv: u8, ply: usize, skipped: bool) {
        if !self.config.use_tt {
            return;
        }
        // A fail-low from a node that skipped moves is not a bound on the position: one of the unsearched
        // moves might beat alpha, and a later visit would cut off on that wrong value.
        if self.config.strict_bounds && skipped && score <= alpha && score > -MATE_BOUND {
            self.stats.skipped_bounds += 1;
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

    /// Orders candidates by the shape class they make, strongest first.
    ///
    /// **Kept as a record of a measurement, not used.** Swapping the per-cell score for class membership
    /// made the exact-search agreement tests go from 6 seconds to over 9 minutes, which is a node-count
    /// explosion: the per-cell score that `Scores::new` computes is better than the class it is derived
    /// from, so ordering cannot be reduced to the class masks alone.
    #[allow(dead_code)]
    fn order_by_class(&self, shapes: &Shapes<Words>, mut cand: Words, side: usize, tt_move: u8, ply: usize) -> MoveList {
        let masks: [(Words, i32); 6] = [
            (shapes.cells(side, threats::FIVE), 900_000),
            (shapes.cells(side, threats::OPEN_FOUR), 500_000),
            (shapes.cells(side, threats::FOUR), 200_000),
            (shapes.cells(side, threats::OPEN_THREE), 50_000),
            (shapes.cells(side, threats::BLOCKED_THREE), 20_000),
            (shapes.cells(side, threats::TWO), 2_000),
        ];
        let killers = self.killers[ply];
        let recent = &self.path[ply.saturating_sub(4)..ply];
        let mut list = MoveList { moves: [Move::default(); SIZE * SIZE], len: 0, next: 0 };
        while let Some(i) = cand.pop_lowest() {
            let cell = i as u8;
            list.moves[list.len] = if cell == tt_move {
                Move { score: TT_MOVE_SCORE, cell, quiet: false }
            } else {
                let mut class = 0;
                for &(mask, value) in masks.iter() {
                    if mask.get(i) {
                        class = value;
                        break;
                    }
                }
                // Moves more than 4 cells from the last two (or four) moves played lose half (three quarters).
                let far = |moves: &[u8]| !moves.is_empty() && moves.iter().all(|&m| distance(i, m as usize) > 4);
                let class = if far(recent) {
                    class / 4
                } else if far(&recent[recent.len().saturating_sub(2)..]) {
                    class / 2
                } else {
                    class
                };
                let killer = killers.iter().position(|&k| k == cell).map_or(0, |p| KILLER_SCORES[p]);
                Move { score: class + killer + self.history[side][i] / HISTORY_DIVISOR, cell, quiet: class == 0 && killer == 0 }
            };
            list.len += 1;
        }
        list
    }

    #[allow(dead_code)]
    fn order_moves(&self, scores: &Scores, mut cand: Words, side: usize, tt_move: u8, ply: usize) -> MoveList {
        let killers = self.killers[ply];
        let recent = &self.path[ply.saturating_sub(4)..ply];
        let mut list = MoveList { moves: [Move::default(); SIZE * SIZE], len: 0, next: 0 };
        while let Some(i) = cand.pop_lowest() {
            let cell = i as u8;
            list.moves[list.len] = if cell == tt_move {
                Move { score: TT_MOVE_SCORE, cell, quiet: false }
            } else {
                let threat = scores.strong[i];
                // Moves more than 4 cells from the last two (or four) moves played lose half (three quarters).
                let far = |moves: &[u8]| !moves.is_empty() && moves.iter().all(|&m| distance(i, m as usize) > 4);
                let shape = scores.shape[i];
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

    #[allow(clippy::too_many_arguments)]
    fn node(&mut self, b: &mut Board, side: usize, depth: i32, mut alpha: i32, beta: i32, ply: usize, stones: u32) -> i32 {
        if self.tick() {
            return 0;
        }
        if ply > self.stats.max_ply {
            self.stats.max_ply = ply;
        }
        let root = ply == 0;
        let shapes = b.shapes();
        let t = Threats::new(&shapes, side);
        if !root && let Some(s) = t.decided(ply, stones) {
            return s;
        }

        // A four against us leaves only its block. Against an open three only its blocks and our own fours
        // are tried, and (with `extend_three_replies`) they cost no depth.
        let opp = 1 - side;
        let block_four = !t.opp_five.is_zero();
        let three_reply = self.config.free_three_replies && !self.plain_iteration && !block_four && !t.opp_open4.is_zero();
        // A four against us leaves only its block, which both references give extra depth: PentaZen
        // one ply, Rapfi 1.3. Free plies are what the cap below limits.
        let four_reply = self.config.free_four_replies && !self.plain_iteration && block_four;
        let forced = block_four || three_reply;
        // A four block is one cell; the reply to an open three is a branching set whose members can be
        // threats themselves, so a free ply there can spend the turn inside one subtree. `no-three-ext`
        // keeps that set and charges the ply: the experiment that separates the set from the depth.
        let extend_three = three_reply && self.config.extend_three_replies;
        let free = (extend_three || four_reply) && ply < self.config.free_ply_limit;
        if !root && depth <= 0 && !free {
            self.stats.horizon_nodes += 1;
            return eval::evaluate_shapes(&shapes, side);
        }
        if forced {
            self.stats.forced_nodes += 1;
        }
        let child_depth = if free { depth } else { depth - 1 };

        // The table is probed once the node is known to be a search node: after the static decisions and the
        // horizon, and before the per-candidate scores, so a cutoff skips the expensive ordering work.
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

        let cand = if block_four {
            t.opp_five
        } else if three_reply {
            // Blocking cells and our own fours, plus our own open threes when `counter_threes`: the
            // answer to an open three is often a counter-threat rather than a passive block.
            let counters = b.open_four_block_cells(opp).or(shapes.cells(side, threats::FOUR));
            if self.config.counter_threes {
                counters.or(shapes.cells(side, threats::OPEN_THREE))
            } else {
                counters
            }
        } else {
            b.candidates()
        };
        let scores = Scores::new(&shapes, side, cand);
        let moves = self.order_moves(&scores, cand, side, tt_move, ply);
        if moves.len == 0 {
            return 0;
        }

        let selective = self.config.selective && !root && !forced;
        // The two pruning mechanisms are separate switches, so each can be measured on its own: the
        // move-count limit moves most of the depth here, the reductions add about a ply on some
        // positions (see `docs/engine-audit.md`, finding 4). Both are read once, outside the loop.
        let prune = selective && self.config.limit;
        let reduce = selective && self.config.reductions;
        let null_window = beta - alpha == 1;
        // "Tactical": either side already has a four or an open three on the board.
        let tactical = !(t.opp_five.is_zero() && t.opp_open4.is_zero() && t.own_open4.is_zero());
        let limit = move_count_limit(depth, tactical);
        let candidates = moves.len as u64;
        let mut searched = 0;
        let mut skipped = false;
        let mut cut = false;
        let orig_alpha = alpha;
        let mut best = -INF;
        let mut best_move = NO_MOVE;
        for (k, m) in moves {
            // Past the limit every move is skipped; one ply from the root only quiet ones are.
            // Nothing is skipped while every move so far loses: one of the skipped moves might not.
            if prune && k >= limit && best > -MATE_BOUND && (ply >= 2 || m.quiet) {
                self.stats.pruned += 1;
                skipped = true;
                continue;
            }
            let reduction = if reduce && m.quiet { late_move_reduction(depth, k, null_window) } else { 0 };
            // A move that makes two threats at once — a double four, or a four and an open three — is
            // searched one ply deeper: it is the shape that usually decides the game.
            let double = self.config.double_threat_ext && {
                let dirs = shapes.ranked(side);
                let mut threats = 0;
                for d in dirs {
                    threats +=
                        (d[threats::FOUR].get(m.cell as usize) || d[threats::OPEN_THREE].get(m.cell as usize)) as i32;
                }
                threats >= 2
            };
            let child = if double { depth } else { child_depth };
            searched += 1;
            self.path[ply] = m.cell;
            let (r, c) = (m.cell as usize / STRIDE, m.cell as usize % STRIDE);
            b.make(side, r, c);
            // After the first move a null window only asks whether the move beats alpha.
            let window = if k == 0 { beta } else { alpha + 1 };
            let mut score = -self.node(b, opp, child - reduction, -window, -alpha, ply + 1, stones + 1);
            if reduction > 0 && score > alpha {
                score = -self.node(b, opp, child, -window, -alpha, ply + 1, stones + 1);
            }
            if window < beta && score > alpha && score < beta {
                score = -self.node(b, opp, child, -beta, -alpha, ply + 1, stones + 1);
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
        self.store_tt(key, depth, best, orig_alpha, beta, best_move, ply, skipped);
        best
    }
}

#[cfg(test)]
mod tests;
