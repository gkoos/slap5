//! The adapter's marshalling and coordinate handling, tested natively: everything here is reachable
//! without a browser except the two JS calls (`JsError::new` in the exported setter, `Function::call4` in
//! the exported search), which is why `decode` and `run` are plain functions.

use super::*;

/// The page's occupancy words for a set of (row, col) cells.
fn words(cells: &[(usize, usize)]) -> [u32; WORDS] {
    let mut words = [0u32; WORDS];
    for &(r, c) in cells {
        let cell = r * SIZE + c;
        words[cell / 32] |= 1 << (cell % 32);
    }
    words
}

fn position(black: &[(usize, usize)], white: &[(usize, usize)], black_to_move: bool) -> Position {
    decode(&words(black), &words(white), black_to_move).expect("a decodable position")
}

#[test]
fn decodes_both_colours_and_the_side_to_move() {
    let p = position(&[(7, 7), (8, 8)], &[(7, 8), (0, 14)], true);
    assert_eq!(p.side, 0, "black to move");
    assert_eq!(p.board.stone_count(), 4);
    for (player, (r, c)) in [(0, (7, 7)), (0, (8, 8)), (1, (7, 8)), (1, (0, 14))] {
        assert!(p.board.stones[player].get(r * STRIDE + c), "({r},{c}) of player {player}");
    }

    let p = position(&[(7, 7)], &[(7, 8)], false);
    assert_eq!(p.side, 1, "white to move");
}

// Cell 224 - row 14, column 14 - is bit 0 of the eighth word, the only bit of that word on the board.
#[test]
fn decodes_the_last_cell_of_the_board() {
    let p = position(&[(14, 14)], &[], true);
    assert!(p.board.stones[0].get(14 * STRIDE + 14));
    assert_eq!(p.board.stone_count(), 1);
}

#[test]
fn rejects_what_the_engine_cannot_represent() {
    assert!(decode(&[0; WORDS], &[0; WORDS - 1], true).is_err(), "seven words");
    assert!(decode(&[0; WORDS + 1], &[0; WORDS], true).is_err(), "nine words");

    let mut padding = [0u32; WORDS];
    padding[7] = 1 << 1; // cell 225, past the last cell of the board
    assert!(decode(&padding, &[0; WORDS], true).is_err(), "padding bit");

    let overlap = words(&[(7, 7)]);
    assert!(decode(&overlap, &overlap, true).is_err(), "both colours on one cell");
}

// The reports have to arrive as (depth, x, y) in the engine's order, once per completed depth, and the
// result's move has to be the last report's - the page plays that report when the user stops the search
// early. The coordinates are checked through `MoveResult`, which is where the row/column swap happens.
#[test]
fn reports_every_completed_iteration_and_returns_the_last_one() {
    let p = position(&[(7, 7), (8, 8)], &[(7, 8), (8, 7)], true);
    let mut seen = Vec::new();
    let mut searcher = Searcher::with_table_bits(12);
    let r = run(&mut searcher, &p, 200.0, 4, &mut |d, x, y, s| seen.push((d, x, y, s))).expect("a move");

    assert_eq!(
        seen.iter().map(|&(d, _, _, _)| d).collect::<Vec<_>>(),
        (1..=r.depth).collect::<Vec<_>>(),
        "one report per completed depth, in order"
    );
    assert_eq!(seen.last().copied(), Some((r.depth, r.x, r.y, r.score)));
    let (x, y) = (r.x, r.y);
    let m = MoveResult::from(r);
    assert_eq!((m.row(), m.col()), (y, x), "the page's row is the engine's y");
    assert!(m.nodes() > 0 && m.depth() >= 1);
}

// A five on the board is decided before the iteration loop, so there is nothing to report and the result
// still carries the winning move - the page's fallback must not be confused with an iteration.
#[test]
fn an_immediately_won_position_reports_nothing() {
    let p = position(&[(7, 3), (7, 4), (7, 5), (7, 6)], &[(10, 10), (11, 10)], true);
    let mut reports = 0;
    let mut searcher = Searcher::with_table_bits(12);
    let r = run(&mut searcher, &p, 100.0, 4, &mut |_, _, _, _| reports += 1).expect("a move");
    assert_eq!(r.score, slap5::search::MATE - 1);
    assert_eq!(r.y, 7, "the winning row");
    assert_eq!(reports, 0);
}

#[test]
fn a_full_board_has_no_move() {
    let (mut black, mut white) = (Vec::new(), Vec::new());
    for r in 0..SIZE {
        for c in 0..SIZE {
            if (r + c) % 2 == 0 {
                black.push((r, c));
            } else {
                white.push((r, c));
            }
        }
    }
    let p = position(&black, &white, true);
    assert_eq!(p.board.stone_count() as usize, CELLS);
    let mut searcher = Searcher::with_table_bits(12);
    assert!(run(&mut searcher, &p, 100.0, 2, &mut |_, _, _, _| {}).is_none());
}
