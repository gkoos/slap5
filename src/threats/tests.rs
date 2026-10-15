use super::*;
use crate::bitboard::{SIZE, STRIDE, U64x4};
use crate::Words;

type Grid = [[u8; SIZE]; SIZE]; // [y][x]: 0 empty, 1 own, 2 other
const DIR4: [(i32, i32); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

fn at(g: &Grid, x: i32, y: i32) -> Option<u8> {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        Some(g[y as usize][x as usize])
    } else {
        None
    }
}

// Own stones adjacent to (x, y) in both directions along (dx, dy).
fn run_arms(g: &Grid, x: i32, y: i32, dx: i32, dy: i32) -> (i32, i32) {
    let count = |sx: i32, sy: i32| {
        let mut n = 0;
        while at(g, x + sx * (n + 1), y + sy * (n + 1)) == Some(1) {
            n += 1;
        }
        n
    };
    (count(-dx, -dy), count(dx, dy))
}

// After a stone at (x, y): the arms and end cells if it sits in a run of exactly four with empty ends.
fn open_four_run(g: &Grid, x: i32, y: i32, dx: i32, dy: i32) -> Option<(i32, i32)> {
    let (back, fwd) = run_arms(g, x, y, dx, dy);
    if 1 + back + fwd != 4 {
        return None;
    }
    let before = at(g, x - (back + 1) * dx, y - (back + 1) * dy);
    let after = at(g, x + (fwd + 1) * dx, y + (fwd + 1) * dy);
    (before == Some(0) && after == Some(0)).then_some((back, fwd))
}

fn placed(g: &Grid, x: i32, y: i32) -> Grid {
    let mut h = *g;
    h[y as usize][x as usize] = 1;
    h
}

fn oracle_five(g: &Grid, x: i32, y: i32) -> bool {
    let h = placed(g, x, y);
    DIR4.iter().any(|&(dx, dy)| {
        let (b, f) = run_arms(&h, x, y, dx, dy);
        1 + b + f >= 5
    })
}

fn oracle_window(g: &Grid, x: i32, y: i32, n: usize) -> bool {
    let h = placed(g, x, y);
    DIR4.iter().any(|&(dx, dy)| {
        (-4..=0).any(|start| {
            let cells: Vec<Option<u8>> =
                (start..start + 5).map(|i| at(&h, x + i * dx, y + i * dy)).collect();
            cells.iter().all(Option::is_some)
                && cells.iter().filter(|c| **c == Some(1)).count() == n
                && cells.iter().filter(|c| **c == Some(0)).count() == 5 - n
        })
    })
}

fn oracle_four(g: &Grid, x: i32, y: i32) -> bool {
    oracle_window(g, x, y, 4)
}

fn oracle_three(g: &Grid, x: i32, y: i32) -> bool {
    oracle_window(g, x, y, 3)
}

fn oracle_two(g: &Grid, x: i32, y: i32) -> bool {
    oracle_window(g, x, y, 2)
}

fn oracle_open_four(g: &Grid, x: i32, y: i32) -> bool {
    let h = placed(g, x, y);
    DIR4.iter().any(|&(dx, dy)| open_four_run(&h, x, y, dx, dy).is_some())
}

fn oracle_open_three(g: &Grid, x: i32, y: i32) -> bool {
    let h = placed(g, x, y);
    DIR4.iter().any(|&(dx, dy)| {
        (-4..=4).filter(|&i| i != 0).any(|i| {
            let (x2, y2) = (x + i * dx, y + i * dy);
            if at(&h, x2, y2) != Some(0) {
                return false;
            }
            let h2 = placed(&h, x2, y2);
            // The new open four must contain the first stone: it lies i steps back from (x2, y2).
            open_four_run(&h2, x2, y2, dx, dy).is_some_and(|(back, fwd)| (-fwd..=back).contains(&i))
        })
    })
}

fn oracle_block_cells(g: &Grid) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            if g[y as usize][x as usize] != 0 {
                continue;
            }
            let h = placed(g, x, y);
            for &(dx, dy) in &DIR4 {
                if let Some((back, fwd)) = open_four_run(&h, x, y, dx, dy) {
                    out.push((x, y));
                    out.push((x - (back + 1) * dx, y - (back + 1) * dy));
                    out.push((x + (fwd + 1) * dx, y + (fwd + 1) * dy));
                }
            }
        }
    }
    out
}

fn board<B: Bits>(g: &Grid) -> Board<B> {
    let mut b = Board::<B>::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            match g[y][x] {
                1 => b.make(0, y, x),
                2 => b.make(1, y, x),
                _ => {}
            }
        }
    }
    b
}

fn cells<B: Bits>(m: B) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            if m.get(y * STRIDE + x) {
                out.push((x as i32, y as i32));
            }
        }
    }
    out
}

fn grid_from(rows: &[&str], x0: usize, y0: usize) -> Grid {
    let mut g = [[0u8; SIZE]; SIZE];
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            g[y0 + dy][x0 + dx] = match ch {
                'X' => 1,
                'O' => 2,
                _ => 0,
            };
        }
    }
    g
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// Noise plus short line seeds (some with gaps), anywhere on the board including edges.
fn random_grid(rng: &mut Rng) -> Grid {
    let mut g = [[0u8; SIZE]; SIZE];
    for _ in 0..rng.below(25) {
        g[rng.below(SIZE)][rng.below(SIZE)] = 1 + rng.below(2) as u8;
    }
    for _ in 0..1 + rng.below(3) {
        let (dx, dy) = DIR4[rng.below(4)];
        let colour = if rng.below(4) == 0 { 2 } else { 1 };
        let (mut x, mut y) = (rng.below(SIZE) as i32, rng.below(SIZE) as i32);
        for _ in 0..2 + rng.below(3) {
            if at(&g, x, y) == Some(0) {
                g[y as usize][x as usize] = colour;
            }
            let step = 1 + rng.below(2) as i32;
            x += dx * step;
            y += dy * step;
        }
    }
    g
}

// Number of positions where each mask was non-empty: five, four, open four, open three, three, two, block.
fn compare<B: Bits>(g: &Grid, seen: &mut [u32; 7]) {
    let b = board::<B>(g);
    let (three, two) = b.three_and_two_cells(0);
    let masks = [
        cells(b.five_cells(0)),
        cells(b.four_cells(0)),
        cells(b.open_four_cells(0)),
        cells(b.open_three_cells(0)),
        cells(three),
        cells(two),
        cells(b.open_four_block_cells(0)),
    ];
    let oracles: [fn(&Grid, i32, i32) -> bool; 6] =
        [oracle_five, oracle_four, oracle_open_four, oracle_open_three, oracle_three, oracle_two];
    for (i, oracle) in oracles.iter().enumerate() {
        let mut expected = Vec::new();
        for y in 0..SIZE as i32 {
            for x in 0..SIZE as i32 {
                if g[y as usize][x as usize] == 0 && oracle(g, x, y) {
                    expected.push((x, y));
                }
            }
        }
        assert_eq!(masks[i], expected, "mask {i} on {g:?}");
        seen[i] += !expected.is_empty() as u32;
    }
    let mut expected = oracle_block_cells(g);
    expected.sort_by_key(|&(x, y)| (y, x));
    expected.dedup();
    assert_eq!(masks[6], expected, "block cells on {g:?}");
    seen[6] += !expected.is_empty() as u32;
}

fn differential<B: Bits>() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut seen = [0u32; 7];
    for _ in 0..3000 {
        let g = random_grid(&mut rng);
        compare::<B>(&g, &mut seen);
    }
    for (i, n) in seen.iter().enumerate() {
        assert!(*n >= 100, "mask {i} was non-empty in only {n} positions; generator too weak");
    }
}

#[test]
fn scalar_words_match_oracle() {
    differential::<U64x4>();
}

#[test]
fn default_words_match_oracle() {
    differential::<Words>();
}

fn set(rows: &[&str], x0: usize, y0: usize, f: fn(&Board<Words>, usize) -> Words) -> Vec<(i32, i32)> {
    cells(f(&board::<Words>(&grid_from(rows, x0, y0)), 0))
}

#[test]
fn open_four_needs_both_ends_empty() {
    let f = |b: &Board<Words>, p| b.open_four_cells(p);
    assert_eq!(set(&[".XXX.."], 3, 7, f), [(3, 7), (7, 7)]);
    assert_eq!(set(&["OXXX.."], 3, 7, f), []);
}

#[test]
fn open_four_is_blocked_by_the_board_edge() {
    let f = |b: &Board<Words>, p| b.open_four_cells(p);
    assert_eq!(set(&["..XXX."], 9, 4, f), [(10, 4)]);
}

#[test]
fn split_gap_makes_five_not_open_four() {
    let g = grid_from(&[".XX.XX."], 3, 7);
    let b = board::<Words>(&g);
    assert_eq!(cells(b.five_cells(0)), [(6, 7)]);
    assert_eq!(cells(b.open_four_cells(0)), []);
}

#[test]
fn open_three_cells_of_an_open_two() {
    let f = |b: &Board<Words>, p| b.open_three_cells(p);
    assert_eq!(set(&["..XX..."], 3, 7, f), [(3, 7), (4, 7), (7, 7), (8, 7)]);
}

#[test]
fn block_cells_of_an_open_three() {
    let f = |b: &Board<Words>, p| b.open_four_block_cells(p);
    assert_eq!(set(&["..XXX.."], 3, 7, f), [(3, 7), (4, 7), (8, 7), (9, 7)]);
}

#[test]
fn four_cells_include_blocked_fours() {
    let f = |b: &Board<Words>, p| b.four_cells(p);
    assert_eq!(set(&["OXXX.."], 3, 7, f), [(7, 7), (8, 7)]);
}

// Directions in the order of `DIRS` (bit 1, 16, 15, 17): right, down, down-left, down-right.
const DIRD: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 1), (1, 1)];

fn for_colour(g: &Grid, colour: u8) -> Grid {
    let mut h = [[0u8; SIZE]; SIZE];
    for y in 0..SIZE {
        for x in 0..SIZE {
            h[y][x] = if g[y][x] == 0 {
                0
            } else if g[y][x] == colour {
                1
            } else {
                2
            };
        }
    }
    h
}

// The shape a stone at (x, y) creates along one direction, or None, with the class precedence:
// five, open four, four, open three, blocked three, two. Defined from the board alone, without the
// mask code, and compared against the per-direction class masks.
fn oracle_class(g: &Grid, x: i32, y: i32, dx: i32, dy: i32) -> Option<usize> {
    let h = placed(g, x, y);
    let (back, fwd) = run_arms(&h, x, y, dx, dy);
    let before = at(&h, x - (back + 1) * dx, y - (back + 1) * dy);
    let after = at(&h, x + (fwd + 1) * dx, y + (fwd + 1) * dy);
    let run = 1 + back + fwd;
    if run >= 5 {
        return Some(FIVE);
    }
    if run == 4 && before == Some(0) && after == Some(0) {
        return Some(OPEN_FOUR);
    }
    // Five-cell windows through (x, y), on the placed board and on the original board.
    let windows = |g: &Grid| -> Vec<Vec<Option<u8>>> {
        (-4..=0)
            .map(|start| (start..start + 5).map(|i| at(g, x + i * dx, y + i * dy)).collect())
            .collect()
    };
    let on_board = |w: &Vec<Option<u8>>| w.iter().all(|c| *c == Some(0) || *c == Some(1));
    let own = |w: &Vec<Option<u8>>| w.iter().filter(|c| **c == Some(1)).count();
    // Four: four own and one empty after the placement, all five cells on board.
    if windows(&h).iter().any(|w| on_board(w) && own(w) == 4) {
        return Some(FOUR);
    }
    // Open three: an empty cell within four steps whose placement makes an open four through (x, y).
    let makes_open_four = (-4..=4).filter(|&i| i != 0).any(|i| {
        if at(&h, x + i * dx, y + i * dy) != Some(0) {
            return false;
        }
        let (x2, y2) = (x + i * dx, y + i * dy);
        let h2 = placed(&h, x2, y2);
        open_four_run(&h2, x2, y2, dx, dy).is_some_and(|(b, f)| (-f..=b).contains(&i))
    });
    if makes_open_four {
        return Some(OPEN_THREE);
    }
    // Blocked three and two: a five-cell window with no opponent and no off-board cell holding
    // exactly two own stones (three) or one (two) on the original board.
    if windows(g).iter().any(|w| on_board(w) && own(w) == 2) {
        return Some(BLOCKED_THREE);
    }
    if windows(g).iter().any(|w| on_board(w) && own(w) == 1) {
        return Some(TWO);
    }
    None
}

// Every empty cell must be in exactly the class the oracle gives, in every direction.
fn compare_classes<B: Bits>(g: &Grid, seen: &mut [u32; SHAPES]) {
    let shapes = board::<B>(g).shapes();
    for player in 0..2 {
        let own_grid = for_colour(g, if player == 0 { 1 } else { 2 });
        for (k, &(dx, dy)) in DIRD.iter().enumerate() {
            for y in 0..SIZE as i32 {
                for x in 0..SIZE as i32 {
                    if own_grid[y as usize][x as usize] != 0 {
                        continue;
                    }
                    let want = oracle_class(&own_grid, x, y, dx, dy);
                    let bit = y as usize * STRIDE + x as usize;
                    for (class, mask) in shapes.ranked(player)[k].iter().enumerate() {
                        assert_eq!(
                            mask.get(bit),
                            want == Some(class),
                            "player {player} dir {k} class {class} at ({x},{y}) on {g:?}"
                        );
                    }
                    if let Some(c) = want {
                        seen[c] += 1;
                    }
                }
            }
        }
    }
}

fn classes_differential<B: Bits>() {
    let mut rng = Rng(0x243F_6A88_85A3_08D3);
    let mut seen = [0u32; SHAPES];
    for _ in 0..600 {
        compare_classes::<B>(&random_grid(&mut rng), &mut seen);
    }
    for (class, n) in seen.iter().enumerate() {
        assert!(*n >= 100, "class {class} appeared {n} times; generator too weak");
    }
}

#[test]
fn scalar_words_classes_match_oracle() {
    classes_differential::<U64x4>();
}

#[test]
fn default_words_classes_match_oracle() {
    classes_differential::<Words>();
}

#[test]
fn classes_are_disjoint_within_a_direction() {
    let mut rng = Rng(0x1319_8A2E_0370_7344);
    for _ in 0..200 {
        let g = random_grid(&mut rng);
        let shapes = board::<Words>(&g).shapes();
        for player in 0..2 {
            for k in 0..4 {
                let mut seen = Words::ZERO;
                for class in 0..SHAPES {
                    let m = shapes.ranked(player)[k][class];
                    assert!(m.and(seen).is_zero(), "player {player} dir {k}: overlap on {g:?}");
                    seen = seen.or(m);
                }
            }
        }
    }
}

#[test]
fn a_double_four_counts_twice() {
    // Three own stones on each arm, the far end of each arm blocked by the opponent: playing the
    // crossing point makes a four in the row and in the column.
    let mut g = [[0u8; SIZE]; SIZE];
    for x in 4..=6 {
        g[7][x] = 1;
    }
    for y in 4..=6 {
        g[y][7] = 1;
    }
    g[7][3] = 2;
    g[3][7] = 2;
    let bit = 7 * STRIDE + 7;
    let shapes = board::<Words>(&g).shapes();
    let dirs = |s: &Shapes<Words>| (0..4).filter(|&k| s.ranked(0)[k][FOUR].get(bit)).count();
    assert_eq!(dirs(&shapes), 2, "the crossing point should make a four in two directions");

    // One arm alone makes one four, so the count is what distinguishes the two positions.
    let mut one = [[0u8; SIZE]; SIZE];
    for x in 4..=6 {
        one[7][x] = 1;
    }
    one[7][3] = 2;
    assert_eq!(dirs(&board::<Words>(&one).shapes()), 1);
}
