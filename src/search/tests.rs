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

fn searcher_with(f: impl FnOnce(&mut Config)) -> Searcher {
    let mut s = searcher();
    f(&mut s.config);
    s
}

// Everything switched off except what `f` turns back on: plain alpha-beta.
fn plain(f: impl FnOnce(&mut Config)) -> Searcher {
    searcher_with(|c| {
        c.use_tt = false;
        c.free_three_replies = false;
        c.leaf_depth = 0;
        c.selective = false;
        f(c);
    })
}

type Rules = fn(&Board, usize, usize, u32) -> Option<i32>;

// The engine's rules for positions decided without search.
fn decided(b: &Board, side: usize, ply: usize, stones: u32) -> Option<i32> {
    Threats::new(b, side).decided(ply, stones)
}

// Only the game's own end: a full board, or a five for the side to move.
fn game_over(b: &Board, side: usize, ply: usize, stones: u32) -> Option<i32> {
    if stones as usize == SIZE * SIZE {
        Some(0)
    } else if !b.five_cells(side).is_zero() {
        Some(MATE - ply as i32 - 1)
    } else {
        None
    }
}

// Every candidate searched, no pruning, ordering or table.
fn minimax(b: &mut Board, side: usize, depth: i32, ply: usize, stones: u32, rules: Rules) -> i32 {
    if let Some(s) = rules(b, side, ply, stones) {
        return s;
    }
    if depth <= 0 {
        return eval::evaluate(b, side);
    }
    let mut best = -INF;
    let mut cand = b.candidates();
    while let Some(i) = cand.pop_lowest() {
        let (r, c) = (i / STRIDE, i % STRIDE);
        b.make(side, r, c);
        best = best.max(-minimax(b, 1 - side, depth - 1, ply + 1, stones + 1, rules));
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

fn agrees_with_minimax(depth: i32, positions: usize, stones: usize, window: usize) {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..positions {
        let b = random_position(&mut rng, stones, window);
        let r = plain(|_| {}).search(&b, 0, limits(depth)).unwrap();
        let mut copy = b;
        let expected = minimax(&mut copy, 0, r.depth, 0, b.stone_count(), decided);
        assert_eq!(r.score, expected, "depth {}", r.depth);
    }
}

#[test]
fn the_decided_positions_are_scored_as_full_minimax_scores_them() {
    let mut rng = Rng(0x0BAD_CAFE_1234_5678);
    let (mut wins, mut losses) = (0, 0);
    for _ in 0..3000 {
        let mut b = random_position(&mut rng, 12, 5);
        let stones = b.stone_count();
        if game_over(&b, 0, 0, stones).is_some() {
            continue;
        }
        let Some(s) = decided(&b, 0, 0, stones) else { continue };
        // A win in 3 plies needs 2 plies of full search to confirm, a loss in 2 needs 1.
        let depth = if s > 0 { 2 } else { 1 };
        assert_eq!(minimax(&mut b, 0, depth, 0, stones, game_over), s);
        if s > 0 { wins += 1 } else { losses += 1 }
    }
    assert!(wins > 20 && losses > 20, "too few decided positions: {wins} wins, {losses} losses");
}

#[test]
fn matches_minimax_at_depth_two() {
    agrees_with_minimax(2, 300, 7, 4);
}

#[test]
fn matches_minimax_at_depth_three() {
    agrees_with_minimax(3, 4, 6, 3);
}

// Reference for the forced-reply and leaf rules: no pruning, ordering or table. `free` is false for the
// plain depth-1 iteration.
fn minimax_free(b: &mut Board, side: usize, depth: i32, ply: usize, stones: u32, free: bool, cfg: &Config) -> i32 {
    let opp = 1 - side;
    if ply > 0 && let Some(s) = decided(b, side, ply, stones) {
        return s;
    }
    let opp_five = b.five_cells(opp);
    let opp_open4 = b.open_four_cells(opp);
    let three_reply = free && cfg.free_three_replies && opp_five.is_zero() && !opp_open4.is_zero();
    let forced = if !opp_five.is_zero() {
        Some(opp_five)
    } else if three_reply {
        Some(b.open_four_block_cells(opp).or(b.four_cells(side)))
    } else {
        None
    };
    let free_ply = three_reply && ply < cfg.free_ply_limit;
    if ply > 0 && depth <= 0 && !free_ply {
        if cfg.leaf_depth > 0 && free {
            return leaf_ref(b, side, cfg.leaf_depth, ply, stones);
        }
        return eval::evaluate(b, side);
    }
    let child_depth = if free_ply { depth } else { depth - 1 };
    let mut best = -INF;
    let mut cand = forced.unwrap_or_else(|| b.candidates());
    while let Some(i) = cand.pop_lowest() {
        let (r, c) = (i / STRIDE, i % STRIDE);
        b.make(side, r, c);
        best = best.max(-minimax_free(b, opp, child_depth, ply + 1, stones + 1, free, cfg));
        b.unmake(side, r, c);
    }
    best
}

// Reference for the leaf search: no pruning. Stand pat or play any four; a forced block is free.
fn leaf_ref(b: &mut Board, side: usize, qd: i32, ply: usize, stones: u32) -> i32 {
    if let Some(s) = decided(b, side, ply, stones) {
        return s;
    }
    let opp = 1 - side;
    if let Some(c) = b.five_cells(opp).pop_lowest() {
        b.make(side, c / STRIDE, c % STRIDE);
        let s = -leaf_ref(b, opp, qd, ply + 1, stones + 1);
        b.unmake(side, c / STRIDE, c % STRIDE);
        return s;
    }
    let stand = eval::evaluate(b, side);
    if qd <= 0 {
        return stand;
    }
    let mut best = stand;
    let mut cand = b.four_cells(side);
    while let Some(i) = cand.pop_lowest() {
        b.make(side, i / STRIDE, i % STRIDE);
        best = best.max(-leaf_ref(b, opp, qd - 1, ply + 1, stones + 1));
        b.unmake(side, i / STRIDE, i % STRIDE);
    }
    best
}

// A low limit keeps the unpruned reference search affordable on dense positions.
const TEST_FREE_LIMIT: usize = 5;
const TEST_LEAF_DEPTH: i32 = 2;

fn agrees_with_free_minimax(depth: i32, positions: usize, cfg: Config) -> (u64, u64) {
    let mut rng = Rng(0x7777_1234_ABCD_0001);
    let (mut forced_nodes, mut leaf_nodes) = (0, 0);
    for _ in 0..positions {
        let b = random_position(&mut rng, 8, 4);
        let r = searcher_with(|c| *c = cfg).search(&b, 0, limits(depth)).unwrap();
        forced_nodes += r.stats.forced_nodes;
        leaf_nodes += r.stats.leaf_nodes;
        let mut copy = b;
        let expected = minimax_free(&mut copy, 0, r.depth, 0, b.stone_count(), r.depth > 1, &cfg);
        assert_eq!(r.score, expected, "depth {} {cfg:?}", r.depth);
    }
    (forced_nodes, leaf_nodes)
}

#[test]
fn matches_minimax_with_free_plies_and_the_leaf_search() {
    // Free-ply and leaf chains make the unpruned reference expensive, so the sample stays small.
    let (depth, positions) = (2, 6);
    let (mut forced, mut leaf_nodes) = (0, 0);
    for (threes, leaf) in [(true, false), (false, true), (true, true)] {
        let cfg = Config {
            use_tt: false,
            free_three_replies: threes,
            free_ply_limit: TEST_FREE_LIMIT,
            leaf_depth: if leaf { TEST_LEAF_DEPTH } else { 0 },
            selective: false,
        };
        let (f, l) = agrees_with_free_minimax(depth, positions, cfg);
        forced += f;
        leaf_nodes += l;
    }
    assert!(forced > 0, "no forced node was reached; the positions do not exercise the rule");
    assert!(leaf_nodes > 0, "no leaf search ran");
}

// Two blocked threes cross at (10,8); (7,8) is a four that forces the block at (7,9) and
// also adds a stone to column 8. Then (10,8) makes two fours at once.
fn chain_of_fours() -> Board {
    board(&[
        (0, 7, 5), (0, 7, 6), (0, 7, 7), (1, 7, 4),
        (0, 8, 8), (0, 9, 8), (1, 6, 8),
        (0, 10, 5), (0, 10, 6), (0, 10, 7), (1, 10, 4),
    ])
}

// A double open three at (7,7).
fn double_three() -> Board {
    board(&[(0, 7, 5), (0, 7, 6), (0, 5, 7), (0, 6, 7), (1, 0, 0), (1, 0, 14)])
}

// Plain depth 2 reaches a position with two fives against the opponent, or an open four for us.
#[test]
fn the_decided_positions_end_a_five_ply_win_at_depth_two() {
    for (b, mv) in [(chain_of_fours(), None), (double_three(), Some((7, 7)))] {
        let mut p = plain(|_| {});
        assert!(p.search(&b, 0, limits(1)).unwrap().score < MATE - 1000);
        let r = p.search(&b, 0, limits(2)).unwrap();
        assert_eq!(r.score, MATE - 5);
        if let Some(mv) = mv {
            assert_eq!((r.x, r.y), mv);
        }
    }
}

#[test]
fn pruning_and_reductions_search_fewer_nodes() {
    let mut rng = Rng(0x5555_AAAA_1234_9876);
    let (mut with, mut without) = (0, 0);
    for _ in 0..4 {
        let b = random_position(&mut rng, 12, 6);
        with += plain(|c| c.selective = true).search(&b, 0, limits(4)).unwrap().stats.nodes;
        without += plain(|_| {}).search(&b, 0, limits(4)).unwrap().stats.nodes;
    }
    assert!(with * 10 < without * 9, "{with} nodes with pruning and reductions, {without} without");
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

fn open_three_win(use_tt: bool) {
    let b = board(&[(0, 7, 5), (0, 7, 6), (0, 7, 7), (1, 0, 0), (1, 0, 14)]);
    let r = searcher_with(|c| c.use_tt = use_tt).search(&b, 0, limits(5)).unwrap();
    assert_eq!(r.score, MATE - 3, "tt {use_tt}");
    assert_eq!(r.y, 7);
    assert!(r.x == 4 || r.x == 8, "x = {}", r.x);
}

#[test]
fn wins_with_an_open_four() {
    open_three_win(true);
    open_three_win(false);
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
