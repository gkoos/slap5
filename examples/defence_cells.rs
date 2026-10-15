//! Measures the open-three defence mask: for each position, the cells `open_four_block_cells` offers
//! against an opponent open three, and how many of those actually remove their open four. The mask is
//! documented as a superset; this says how much of it is dead weight before anyone replaces it with an
//! exact generator. Not part of the engine.
use high5::bitboard::Bits;
use high5::Board;

fn main() {
    for line in include_str!("../data/audit_followup.txt").lines().filter(|l| !l.starts_with('#')) {
        let (name, rest) = line.split_once(" us=1 stones=").unwrap();
        let stones: Vec<(usize, usize, usize)> = rest
            .split_whitespace()
            .map(|s| {
                let (p, xy) = s.split_once(':').unwrap();
                let (x, y) = xy.split_once(',').unwrap();
                (p.parse().unwrap(), x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        let mut board = Board::new();
        for (p, x, y) in stones {
            board.make(p, y, x);
        }
        for side in [0, 1] {
            let opp = 1 - side;
            if board.open_four_cells(opp).is_zero() {
                continue;
            }
            let mask = board.open_four_block_cells(opp);
            let mut real = 0;
            let mut temps = 0;
            for i in 0..225usize {
                if !mask.get(i) {
                    continue;
                }
                let mut after = board;
                after.make(side, i / 16, i % 16);
                if after.open_four_cells(opp).is_zero() {
                    real += 1;
                } else {
                    temps += 1;
                }
            }
            println!(
                "{name} side={side}: mask {} cells, {real} remove their open four, {temps} do not",
                mask.count_ones()
            );
        }
    }
}
