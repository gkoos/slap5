//! Threat masks over empty cells, from the point of view of the player owning `own`.
//! Freestyle rules: five or more in a row wins. Every mask is a subset of `empty`.

use crate::bitboard::{Bits, Board, DIRS};

/// Bit s of `o[j]`/`e[j]` is the own/empty state of the cell j steps after cell s along `d`.
#[inline(always)]
fn lines<B: Bits>(own: B, empty: B, d: u32) -> ([B; 6], [B; 6]) {
    let mut o = [B::ZERO; 6];
    let mut e = [B::ZERO; 6];
    for j in 0..6 {
        o[j] = own.shr(j as u32 * d);
        e[j] = empty.shr(j as u32 * d);
    }
    (o, e)
}

/// Cells where placing a stone makes five or more in a row.
pub fn five_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        for k in 0..5 {
            let mut m = e[k];
            for j in 0..5 {
                if j != k {
                    m = m.and(o[j]);
                }
            }
            res = res.or(m.shl(k as u32 * d));
        }
    }
    res
}

/// Cells where placing a stone leaves a window of five with four own stones and one empty cell.
pub fn four_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        for k in 0..5 {
            for hole in 0..5 {
                if hole == k {
                    continue;
                }
                let mut m = e[k].and(e[hole]);
                for j in 0..5 {
                    if j != k && j != hole {
                        m = m.and(o[j]);
                    }
                }
                res = res.or(m.shl(k as u32 * d));
            }
        }
    }
    res
}

/// Cells where placing a stone makes a run of exactly four with both ends empty.
pub fn open_four_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        let ends = e[0].and(e[5]);
        for k in 1..=4 {
            let mut m = ends.and(e[k]);
            for j in 1..=4 {
                if j != k {
                    m = m.and(o[j]);
                }
            }
            res = res.or(m.shl(k as u32 * d));
        }
    }
    res
}

/// Cells where placing a stone makes an open four possible next move using that stone:
/// a six-cell window with empty ends whose middle four hold two own stones and two empty cells.
pub fn open_three_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        let ends = e[0].and(e[5]);
        for k in 1..=4 {
            for l in (k + 1)..=4 {
                let mut m = ends.and(e[k]).and(e[l]);
                for j in 1..=4 {
                    if j != k && j != l {
                        m = m.and(o[j]);
                    }
                }
                res = res.or(m.shl(k as u32 * d)).or(m.shl(l as u32 * d));
            }
        }
    }
    res
}

/// Both ends and the middle gap of every window `open_four_cells` would use. A superset of the
/// replies that stop an open four: a far end can be listed although taking it does not help.
pub fn open_four_block_cells<B: Bits>(own: B, empty: B) -> B {
    let mut res = B::ZERO;
    for d in DIRS {
        let (o, e) = lines(own, empty, d);
        let ends = e[0].and(e[5]);
        for k in 1..=4 {
            let mut m = ends.and(e[k]);
            for j in 1..=4 {
                if j != k {
                    m = m.and(o[j]);
                }
            }
            res = res.or(m).or(m.shl(5 * d)).or(m.shl(k as u32 * d));
        }
    }
    res
}

impl<B: Bits> Board<B> {
    pub fn five_cells(&self, player: usize) -> B {
        five_cells(self.stones[player], self.empty())
    }

    pub fn four_cells(&self, player: usize) -> B {
        four_cells(self.stones[player], self.empty())
    }

    pub fn open_four_cells(&self, player: usize) -> B {
        open_four_cells(self.stones[player], self.empty())
    }

    pub fn open_three_cells(&self, player: usize) -> B {
        open_three_cells(self.stones[player], self.empty())
    }

    pub fn open_four_block_cells(&self, player: usize) -> B {
        open_four_block_cells(self.stones[player], self.empty())
    }
}

#[cfg(test)]
mod tests {
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

    fn oracle_four(g: &Grid, x: i32, y: i32) -> bool {
        let h = placed(g, x, y);
        DIR4.iter().any(|&(dx, dy)| {
            (-4..=0).any(|start| {
                let cells: Vec<Option<u8>> =
                    (start..start + 5).map(|i| at(&h, x + i * dx, y + i * dy)).collect();
                cells.iter().all(Option::is_some)
                    && cells.iter().filter(|c| **c == Some(1)).count() == 4
                    && cells.iter().filter(|c| **c == Some(0)).count() == 1
            })
        })
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

    // Number of positions where each mask was non-empty: five, four, open four, open three, block.
    fn compare<B: Bits>(g: &Grid, seen: &mut [u32; 5]) {
        let b = board::<B>(g);
        let masks = [
            cells(b.five_cells(0)),
            cells(b.four_cells(0)),
            cells(b.open_four_cells(0)),
            cells(b.open_three_cells(0)),
            cells(b.open_four_block_cells(0)),
        ];
        let oracles: [fn(&Grid, i32, i32) -> bool; 4] =
            [oracle_five, oracle_four, oracle_open_four, oracle_open_three];
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
        assert_eq!(masks[4], expected, "block cells on {g:?}");
        seen[4] += !expected.is_empty() as u32;
    }

    fn differential<B: Bits>() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut seen = [0u32; 5];
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
}
