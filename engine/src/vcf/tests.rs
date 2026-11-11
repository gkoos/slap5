use super::*;

fn board(stones: &[(usize, usize, usize)]) -> Board {
    let mut b = Board::new();
    for &(p, r, c) in stones {
        b.make(p, r, c);
    }
    b
}

fn room() -> Budget {
    Budget::new(200_000)
}

// A five is available now.
#[test]
fn a_five_available_is_mate_in_one() {
    let mut b = board(&[(0, 7, 5), (0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0)]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut room()), Some(MATE - 1));
}

// Either end of the three makes an open four, which one block cannot cover.
#[test]
fn an_open_four_made_now_is_mate_in_three() {
    let mut b = board(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut room()), Some(MATE - 3));
}

// The position the search tests score as a five-ply win, proved here with no search around it.
#[test]
fn the_chain_of_fours_is_proved_at_the_same_distance() {
    let mut b = board(&[
        (0, 7, 5), (0, 7, 6), (0, 7, 7), (1, 7, 4),
        (0, 8, 8), (0, 9, 8), (1, 6, 8),
        (0, 10, 5), (0, 10, 6), (0, 10, 7), (1, 10, 4),
    ]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut room()), Some(MATE - 5));
}

// They can complete a five of their own, so our four forces nothing and the chain dies there. This
// is the position family the search used to turn into a mate it had not proved.
#[test]
fn a_defender_five_kills_the_chain() {
    let mut b = board(&[(0, 2, 2), (0, 3, 3), (0, 4, 4), (1, 7, 7), (1, 7, 8), (1, 7, 9), (1, 7, 10)]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut room()), None);
}

// Nothing forcing: the quiet answer, not an invented one.
#[test]
fn a_quiet_position_proves_nothing() {
    let mut b = board(&[(0, 7, 7), (1, 7, 8), (0, 6, 6)]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut room()), None);
}

// The budget is what ends a walk that will not finish; running out is silent.
#[test]
fn an_empty_budget_stops_the_walk() {
    let mut b = board(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)]);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut Budget::new(0)), None);
}

// The walk counts the nodes it actually entered. With no deadline and a budget it never exhausts the
// two agree; the case that matters is a deadline cut, which must not be reported as the whole budget.
#[test]
fn visited_counts_the_nodes_entered_and_not_the_budget() {
    let mut b = board(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)]);
    let mut budget = Budget::new(200_000);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut budget), Some(MATE - 3));
    assert!(budget.visited > 0);
    assert_eq!(budget.nodes, 200_000 - budget.visited, "a node spent is a node entered");
}

#[test]
fn a_deadline_cut_reports_only_the_nodes_entered() {
    // 1024 is the deadline-check interval, so the first node sees the clock and the walk stops there:
    // one node entered against a budget of 1024.
    let mut b = board(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)]);
    let mut budget = Budget::new(1024).with_deadline(Instant::now());
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut budget), None);
    assert_eq!(budget.visited, 1);
}

// Resetting a reused table clears it and the counters, so a later position sees what a fresh
// allocation would have given it: the equivalence a table reused across turns must preserve.
#[test]
fn reset_clears_the_table() {
    let stones = [(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)];
    let key = board(&stones).hash();
    let mut budget = Budget::new(200_000);
    let mut b = board(&stones);
    assert_eq!(vcf(&mut b, 0, 0, 8, &mut budget), Some(MATE - 3));
    assert!(budget.probe(key).is_some(), "the proof should have left an entry");
    budget.reset(200_000);
    assert!(budget.probe(key).is_none(), "reset must clear the entry");
    assert_eq!(budget.visited, 0);
}

// --- equivalence corpus -----------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// A fixed set of positions: the unit cases above plus tension and random ones, so the fingerprint
// below covers proofs, cut nodes, table swaps and quiet returns.
fn corpus() -> Vec<Board> {
    let mut v = vec![
        board(&[(0, 7, 5), (0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0)]),
        board(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 0, 0), (1, 0, 14)]),
        board(&[
            (0, 7, 5), (0, 7, 6), (0, 7, 7), (1, 7, 4),
            (0, 8, 8), (0, 9, 8), (1, 6, 8),
            (0, 10, 5), (0, 10, 6), (0, 10, 7), (1, 10, 4),
        ]),
        board(&[(0, 2, 2), (0, 3, 3), (0, 4, 4), (1, 7, 7), (1, 7, 8), (1, 7, 9), (1, 7, 10)]),
        board(&[(0, 7, 5), (0, 7, 6), (0, 5, 7), (0, 6, 7), (1, 0, 0), (1, 0, 14)]),
        board(&[(0, 7, 6), (0, 7, 7), (0, 8, 8), (1, 7, 8), (1, 6, 6)]),
    ];
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..8 {
        let mut b = Board::new();
        let (r0, c0) = (4 + rng.next() as usize % 4, 4 + rng.next() as usize % 4);
        let mut placed = Vec::new();
        while placed.len() < 14 {
            let cell = (r0 + rng.next() as usize % 5, c0 + rng.next() as usize % 5);
            if !placed.contains(&cell) {
                b.make(placed.len() % 2, cell.0, cell.1);
                placed.push(cell);
            }
        }
        v.push(b);
    }
    v
}

/// (proved, score, root best move, nodes entered) for one position and side, with no deadline.
fn fingerprint(b: &Board, side: usize, depth: i32) -> (bool, i32, i32, i32) {
    let key = b.hash() ^ if side == 1 { zobrist::SIDE } else { 0 };
    let mut board = *b;
    let mut budget = Budget::new(200_000);
    let won = vcf(&mut board, side, 0, depth, &mut budget);
    let best = budget.probe(key).map(|(_, _, m)| m as i32).unwrap_or(-1);
    (won.is_some(), won.unwrap_or(0), best, budget.visited)
}

// The prover's fingerprint over the corpus: proved, distance, root move, nodes entered. Any change
// meant to be an equivalence - cheaper masks, a fixed buffer, no `wins` after the block - must leave
// every tuple identical. The values were captured against the revision before that unpacking work.
#[test]
fn the_corpus_is_proved_identically() {
    let mut got = Vec::new();
    for b in corpus() {
        for side in 0..2 {
            got.push(fingerprint(&b, side, 8));
        }
    }
    #[rustfmt::skip]
    let expected: Vec<(bool, i32, i32, i32)> = vec![
        (true, 29999, -1, 1), (false, 0, 0, 1),
        (true, 29997, 117, 1), (false, 0, 0, 1),
        (true, 29995, 120, 2), (false, 0, 0, 1),
        (false, 0, 0, 1), (true, 29999, -1, 1),
        (false, 0, 0, 1), (false, 0, 0, 1),
        (false, 0, 0, 1), (false, 0, 0, 1),
        (false, 0, 0, 1), (true, 29999, -1, 1),
        (true, 29999, -1, 1), (false, 0, 0, 1),
        (true, 29997, 105, 1), (false, 0, 0, 3),
        (false, 0, 0, 5), (false, 0, 0, 1),
        (false, 0, 0, 1), (true, 29997, 119, 1),
        (false, 0, 0, 3), (true, 29995, 118, 5),
        (false, 0, 0, 1), (false, 0, 0, 1),
        (true, 29997, 134, 1), (true, 29997, 118, 1),
    ];
    assert_eq!(got, expected);
}

