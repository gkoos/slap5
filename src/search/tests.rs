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
        c.free_four_replies = false;
        c.double_threat_ext = false;
        c.selective = false;
        f(c);
    })
}

type Rules = fn(&Board, usize, usize, u32) -> Option<i32>;

// The engine's rules for positions decided without search.
fn decided(b: &Board, side: usize, ply: usize, stones: u32) -> Option<i32> {
    Threats::new(&b.shapes(), side).decided(ply, stones)
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
        let mut counters = b.open_four_block_cells(opp).or(b.four_cells(side));
        if cfg.counter_threes {
            counters = counters.or(b.open_three_cells(side));
        }
        Some(counters)
    } else {
        None
    };
    let free_ply = three_reply && cfg.extend_three_replies && ply < cfg.free_ply_limit;
    if ply > 0 && depth <= 0 && !free_ply {
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

// Plain depth 2 reaches a position with two fives against the opponent, or an open four for us.


// A low limit keeps the unpruned reference search affordable on dense positions.
const TEST_FREE_LIMIT: usize = 5;

fn agrees_with_free_minimax(depth: i32, positions: usize, cfg: Config) -> (u64, u64) {
    let mut rng = Rng(0x7777_1234_ABCD_0001);
    let (mut forced_nodes, mut horizon_nodes) = (0, 0);
    for _ in 0..positions {
        let b = random_position(&mut rng, 8, 4);
        let r = searcher_with(|c| *c = cfg).search(&b, 0, limits(depth)).unwrap();
        forced_nodes += r.stats.forced_nodes;
        horizon_nodes += r.stats.horizon_nodes;
        let mut copy = b;
        let expected = minimax_free(&mut copy, 0, r.depth, 0, b.stone_count(), r.depth > 1, &cfg);
        assert_eq!(r.score, expected, "depth {} {cfg:?}", r.depth);
    }
    (forced_nodes, horizon_nodes)
}

#[test]
fn matches_minimax_with_free_plies() {
    // Free-ply chains make the unpruned reference expensive, so the sample stays small. The reference
    // models free replies to an open three, which is what this test exercises.
    let (depth, positions) = (2, 6);
    let (mut forced, mut horizon) = (0, 0);
    for (threes, ext) in [(true, true), (true, false), (false, false)] {
        let cfg = Config {
            use_tt: false,
            free_three_replies: threes,
            extend_three_replies: ext,
            free_four_replies: false,
            double_threat_ext: false,
            free_ply_limit: TEST_FREE_LIMIT,
            selective: false,
            ..Config::default()
        };
        let (f, l) = agrees_with_free_minimax(depth, positions, cfg);
        forced += f;
        horizon += l;
    }
    assert!(horizon > 0, "no horizon node was reached");
    assert!(forced > 0, "no forced node was reached; the positions do not exercise the rule");
}

// The examples and the suite reach the settings only through `Config::apply`, so the switches have to
// be the switches.
#[test]
fn the_config_switches_do_what_they_say() {
    let mut c = Config::default();
    assert!(c.apply("no-threes"));
    assert!(!c.free_three_replies && c.free_four_replies && c.extend_three_replies);
    assert!(c.apply("no-three-ext"));
    assert!(!c.extend_three_replies);
    assert!(!c.lmr_table, "the table is off until it has been measured");
    assert!(c.apply("lmr-table"));
    assert!(c.lmr_table);
    assert!(c.apply("no-lmr-table"));
    assert!(!c.lmr_table);
    assert!(!c.iir, "IIR is off until it has been measured");
    assert!(c.apply("iir"));
    assert!(c.iir);
    assert!(c.apply("no-iir"));
    assert!(!c.iir);
    assert!(c.null_move, "null-move pruning is on by default since 16 October 2026");
    assert!(c.apply("no-null-move"));
    assert!(!c.null_move);
    assert!(c.apply("null-move"));
    assert!(c.null_move);
    assert!(c.apply("cap=16"));
    assert_eq!(c.free_ply_limit, 16);
    assert!(c.apply("verify=3"));
    assert_eq!(c.verify_ply_limit, 3);
    assert!(!c.apply("verify=x"), "verify without a number is not a switch");
    assert!(!c.apply("cap=x"), "cap without a number is not a switch");
    assert!(!c.apply("nonsense"));
    assert_eq!(c.free_ply_limit, 16, "a rejected switch must not change anything");
}

// The table is PentaZen's curve at our resolution: more reduction deeper in the move list and at greater
// depth, the PV column one ply lower, and never more reduction than leaves the child one ply to search.
// It is off unless `lmr-table` is on, so this does not describe the default.
#[test]
fn the_reduction_table_curves_and_leaves_a_ply() {
    for pv in [false, true] {
        for depth in 3..=12 {
            let mut previous = 0;
            for k in 3..=LMR_MAX_MOVES {
                let r = late_move_reduction_table(depth, depth - 1, k, pv);
                assert!(r >= previous, "not monotone: depth {depth}, move {k}");
                assert!(r <= depth - 2, "below one ply: depth {depth}, move {k}, reduction {r}");
                previous = r;
            }
        }
        assert_eq!(late_move_reduction_table(2, 1, 10, pv), 0, "shallow nodes are not reduced");
        assert_eq!(late_move_reduction_table(9, 8, 2, pv), 0, "the first moves are not reduced");
    }
    for depth in 3..=12 {
        for k in 3..=LMR_MAX_MOVES {
            let non_pv = late_move_reduction_table(depth, depth - 1, k, false);
            let pv = late_move_reduction_table(depth, depth - 1, k, true);
            assert!(pv <= non_pv && non_pv - pv <= 1, "depth {depth}, move {k}");
        }
    }
    // A different curve from the flat formula it replaces, not the same thing under another name.
    assert_ne!(late_move_reduction_table(8, 7, 12, false), super::late_move_reduction(8, 12, false));
}

// IIR trades a ply of child depth for ordering at nodes whose table probe gave no move. Forced nodes are
// never reduced, so a chain of fours must reach its mate either way; a quiet position with the table off is
// what makes the reduction fire, and the counter is the only way a log can see that it did.
#[test]
fn iir_is_counted_and_does_not_lose_a_mate() {
    for (name, b) in [("chain of fours", chain_of_fours()), ("double three", double_three())] {
        let mut s = searcher_with(|c| {
            c.iir = true;
            c.verify_ply_limit = 0;
        });
        let r = s.search(&b, 0, limits(4)).unwrap();
        assert!(r.score >= MATE_BOUND, "{name}: the mate survives IIR");
    }
    let mut off = searcher_with(|c| c.use_tt = false);
    let mut on = searcher_with(|c| {
        c.use_tt = false;
        c.iir = true;
    });
    let quiet = board(&[(0, 7, 7), (1, 7, 8), (0, 8, 8)]);
    off.search(&quiet, 0, limits(5)).unwrap();
    on.search(&quiet, 0, limits(5)).unwrap();
    assert_eq!(off.stats.iir_nodes, 0, "off by default");
    assert!(on.stats.iir_nodes > 0, "the reduction fired: {} nodes", on.stats.iir_nodes);
}

// Null-move pruning must not lose a mate that the same search finds without it, and it has to fire somewhere;
// the counter is the only way a log can see the pass being tried.
#[test]
fn null_move_keeps_the_mates_and_is_counted() {
    for (name, b) in [("chain of fours", chain_of_fours()), ("double three", double_three())] {
        let mut s = searcher_with(|c| {
            c.null_move = true;
            c.verify_ply_limit = 0;
        });
        let r = s.search(&b, 0, limits(4)).unwrap();
        assert!(r.score >= MATE_BOUND, "{name}: the mate survives null-move pruning");
    }
    let quiet = board(&[(0, 7, 7), (1, 7, 8), (0, 8, 8), (1, 8, 7)]);
    let mut off = searcher_with(|c| c.null_move = false);
    let mut on = searcher_with(|c| {
        c.null_move = true;
        c.verify_ply_limit = 0;
    });
    off.search(&quiet, 0, limits(5)).unwrap();
    on.search(&quiet, 0, limits(5)).unwrap();
    assert_eq!(off.stats.null_nodes, 0, "off by default");
    assert!(on.stats.null_nodes > 0, "the pass was tried: {} nodes", on.stats.null_nodes);
}

// What null-move pruning saves and what it costs, at fixed depth with the mate check off so nothing depends
// on the clock: nodes with the pass off against on, plus how often it was tried and how often it cut. The cut
// rate is the number that decides this mechanism: a pass that does not cut still searched a shallower tree,
// so a modest rate can already pay. Run with `-- --nocapture` to read the table.
#[test]
fn null_move_saves_subtrees_or_only_costs_nodes() {
    let positions = [
        ("chain of fours", chain_of_fours()),
        ("double three", double_three()),
        ("quiet a", board(&[(0, 7, 7), (1, 7, 8), (0, 8, 8)])),
        ("quiet b", board(&[(0, 7, 7), (1, 8, 8), (0, 7, 9), (1, 6, 8)])),
        ("midgame", board(&[(0, 7, 7), (1, 7, 8), (0, 8, 8), (1, 6, 8), (0, 8, 7), (1, 9, 8)])),
    ];
    for (name, b) in positions {
        let run = |null_move: bool| {
            let mut s = searcher_with(|c| {
                c.null_move = null_move;
                c.verify_ply_limit = 0;
            });
            s.search(&b, 0, limits(6)).unwrap();
            (s.stats.nodes, s.stats.null_nodes, s.stats.null_cutoffs)
        };
        let (off, _, _) = run(false);
        let (on, tried, cut) = run(true);
        println!(
            "{name:<16} nodes off {off:>9} on {on:>9} saved {:>6.1}%  passes {tried:>6} cuts {cut:>6}",
            (off as f64 - on as f64) * 100.0 / off as f64
        );
    }
}

// A reported mate says whether the table-free check ran, which is the only way a log can tell a proven
// mate from a claimed one (see `docs/design-decisions.md`).
#[test]
fn a_reported_mate_says_whether_it_was_proved() {
    for (name, b) in [("chain of fours", chain_of_fours()), ("double three", double_three())] {
        // The check is bounded in time, so the test gives it enough of it to finish proving a mate in 5.
        let mut s = searcher_with(|c| c.verify_ms = 5_000);
        let r = s.search(&b, 0, limits(2)).unwrap();
        assert!(r.score >= MATE_BOUND, "{name}");
        assert_eq!(r.mate_verified, Some(true), "{name}: a mate in 5 is short enough to prove without the table");
        assert!(r.stats.verify_nodes > 0, "{name}: the check ran");
    }
    // Past `verify_ply_limit` the check cannot finish inside a turn, so the mate is reported unverified.
    let mut s = searcher_with(|c| c.verify_ply_limit = 0);
    let r = s.search(&double_three(), 0, limits(2)).unwrap();
    assert!(r.score >= MATE_BOUND);
    assert_eq!(r.mate_verified, Some(false));
    assert_eq!(r.stats.verify_nodes, 0, "no check was attempted");
    // With no mate there is nothing to verify.
    let mut s = searcher();
    let b = board(&[(0, 7, 7), (1, 7, 8), (0, 8, 8)]);
    assert_eq!(s.search(&b, 0, limits(1)).unwrap().mate_verified, None);
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

// The same two positions with the settings the engine actually plays with. `double_three` cannot be
// won by fours alone: the winning move makes an open three, the answer to it is forced and free, and
// the win only appears after that. `chain_of_fours` covers the four half. If threat handling breaks,
// these fail even though every other test still passes.
#[test]
fn the_default_settings_prove_wins_that_need_an_open_three() {
    let positions = [("chain of fours", chain_of_fours(), None), ("double three", double_three(), Some((7, 7)))];
    for (name, b, mv) in positions {
        let mut s = searcher();
        let r = s.search(&b, 0, limits(2)).unwrap();
        assert!(r.score >= MATE_BOUND, "{name}: expected a proven win, got {} at depth {}", r.score, r.depth);
        if let Some(mv) = mv {
            assert_eq!((r.x, r.y), mv, "{name}: wrong move");
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

// A fail-low from a node that skipped moves must not be stored as a bound on the position. The guard
// returns before the store, so its counter is the evidence that the path is reachable at all.
#[test]
fn a_fail_low_is_not_stored_when_moves_were_skipped() {
    let mut rng = Rng(0x1234_ABCD_5678_EF01);
    let mut skipped = 0;
    for _ in 0..10 {
        let b = random_position(&mut rng, 20, 7);
        let r = searcher_with(|c| c.strict_bounds = true).search(&b, 0, limits(5)).unwrap();
        skipped += r.stats.skipped_bounds;
    }
    assert!(skipped > 0, "the guard never fired; the sample does not exercise the move-count limit");
}

// A mate ends the iteration loop and decides the move, so it has to be true. The engine re-searches it
// with the limit and the reductions off; this checks the result against an independent exact search.
#[test]
fn mates_are_confirmed_by_an_exact_search() {
    let mut rng = Rng(0x5EED_0F1E_2D3C_4B5A);
    let mut mates = 0;
    for _ in 0..40 {
        let b = random_position(&mut rng, 14, 6);
        let r = searcher_with(|_| {}).search(&b, 0, limits(2)).unwrap();
        if r.score.abs() < MATE_BOUND {
            continue;
        }
        mates += 1;
        let exact = searcher_with(|c| c.selective = false).search(&b, 0, limits(r.depth)).unwrap();
        assert!(
            exact.score.abs() >= MATE_BOUND && exact.score.signum() == r.score.signum(),
            "reported {} but the exact search says {}",
            r.score,
            exact.score
        );
    }
    assert!(mates > 0, "no mate was reached in the sample");
}
