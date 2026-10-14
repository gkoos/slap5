#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
mod avx;
pub mod bitboard;
pub mod game;
pub mod mover;
pub mod protocol;
pub mod threats;

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
pub type Words = avx::U256;
#[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
pub type Words = bitboard::U64x4;

pub type Board = bitboard::Board<Words>;

#[cfg(test)]
mod tests {
    use crate::bitboard::{self, Bits, SIZE, U64x4};
    use crate::Board;

    fn random_moves(seed: u64, n: usize) -> Vec<(usize, usize, usize)> {
        let mut s = seed | 1;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        while out.len() < n {
            let r = 3 + (next() % 9) as usize;
            let c = 3 + (next() % 9) as usize;
            if seen.insert((r, c)) {
                out.push((out.len() % 2, r, c));
            }
        }
        out
    }

    // Board uses the AVX2 words where available; U64x4 is the scalar reference.
    #[test]
    fn matches_scalar_reference() {
        for seed in 1..400u64 {
            let n = 10 + (seed as usize % 50);
            let mut a = Board::new();
            let mut b = bitboard::Board::<U64x4>::new();
            for (p, r, c) in random_moves(seed, n) {
                a.make(p, r, c);
                b.make(p, r, c);
            }
            for p in 0..2 {
                assert_eq!(a.wins(p), b.wins(p), "seed {seed}");
                assert_eq!(a.five_cells(p).count_ones(), b.five_cells(p).count_ones(), "seed {seed}");
            }
        }
    }

    #[test]
    fn known_positions() {
        let mut a = Board::new();
        for c in 3..7 {
            a.make(0, 5, c);
        }
        assert!(!a.wins(0));
        assert_eq!(a.five_cells(0).count_ones(), 2);
        a.make(0, 5, 7);
        assert!(a.wins(0));
        assert!(!a.wins(1));
    }

    #[test]
    fn no_wrap_across_row_edge() {
        let mut a = Board::new();
        for c in 11..SIZE {
            a.make(0, 4, c);
        }
        for c in 0..2 {
            a.make(0, 5, c);
        }
        assert!(!a.wins(0));
        assert_eq!(a.five_cells(0).count_ones(), 1);
    }

    #[test]
    fn unmake_restores() {
        let mut a = Board::new();
        a.make(1, 7, 7);
        a.unmake(1, 7, 7);
        assert_eq!(a.five_cells(1).count_ones(), 0);
        assert!(!a.wins(1));
    }
}
