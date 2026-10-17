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

fn candidates_match_brute_force<B: Bits>() {
    let mut s = 0x7F4A_7C15_9E37_79B9u64;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..300 {
        let mut b = bitboard::Board::<B>::new();
        let mut stones = Vec::new();
        for _ in 0..1 + (next() % 6) as usize {
            let (r, c) = ((next() % SIZE as u64) as usize, (next() % SIZE as u64) as usize);
            if !stones.contains(&(r, c)) {
                b.make(stones.len() % 2, r, c);
                stones.push((r, c));
            }
        }
        let mut mask = b.candidates();
        let mut got = Vec::new();
        while let Some(i) = mask.pop_lowest() {
            got.push((i / bitboard::STRIDE, i % bitboard::STRIDE));
        }
        let mut want = Vec::new();
        for r in 0..SIZE {
            for c in 0..SIZE {
                let near = stones.iter().any(|&(sr, sc)| r.abs_diff(sr).max(c.abs_diff(sc)) <= 2);
                if near && !stones.contains(&(r, c)) {
                    want.push((r, c));
                }
            }
        }
        assert_eq!(got, want, "stones {stones:?}");
    }
}

#[test]
fn candidates_match_brute_force_scalar() {
    candidates_match_brute_force::<U64x4>();
}

#[test]
fn candidates_match_brute_force_default() {
    candidates_match_brute_force::<crate::Words>();
}

#[test]
fn no_candidates_on_an_empty_board() {
    assert!(Board::new().candidates().is_zero());
}

fn recomputed_hash<B: Bits>(b: &bitboard::Board<B>) -> u64 {
    let mut h = 0;
    for p in 0..2 {
        for r in 0..SIZE {
            for c in 0..SIZE {
                if b.stones[p].get(r * bitboard::STRIDE + c) {
                    h ^= crate::zobrist::key(p, r * bitboard::STRIDE + c);
                }
            }
        }
    }
    h
}

fn hash_follows_make_and_unmake<B: Bits>() {
    for seed in 1..200u64 {
        let moves = random_moves(seed, 10 + (seed as usize % 40));
        let mut b = bitboard::Board::<B>::new();
        assert_eq!(b.hash(), 0);
        for &(p, r, c) in &moves {
            b.make(p, r, c);
            assert_eq!(b.hash(), recomputed_hash(&b), "make, seed {seed}");
        }
        for &(p, r, c) in moves.iter().rev().step_by(2) {
            b.unmake(p, r, c);
            assert_eq!(b.hash(), recomputed_hash(&b), "unmake, seed {seed}");
        }
    }
}

#[test]
fn hash_follows_make_and_unmake_scalar() {
    hash_follows_make_and_unmake::<U64x4>();
}

#[test]
fn hash_follows_make_and_unmake_default() {
    hash_follows_make_and_unmake::<crate::Words>();
}

// `Words` is picked at compile time: `target_feature = "avx2"` comes from the `target-cpu=native` that
// `.cargo/config.toml` sets, and Cargo finds that file by walking up from the *current directory* rather
// than from the manifest. A build that misses it falls back to the scalar words silently, and the scalar
// board measured 3-4x slower on the mask operations the search is made of, so a measurement taken on one
// is meaningless. Where the CPU has AVX2 the build must have selected it; where it does not, the scalar
// board is the intended fallback and there is nothing to check.
#[cfg(target_arch = "x86_64")]
#[test]
fn avx2_words_are_selected_where_available() {
    if std::arch::is_x86_feature_detected!("avx2") {
        assert!(
            cfg!(target_feature = "avx2"),
            "built without -C target-cpu=native: the scalar board is in use (see .cargo/config.toml)"
        );
    }
}

#[test]
fn hash_ignores_move_order_but_not_colour() {
    let mut a = Board::new();
    let mut b = Board::new();
    a.make(0, 3, 3);
    a.make(1, 4, 4);
    b.make(1, 4, 4);
    b.make(0, 3, 3);
    assert_eq!(a.hash(), b.hash());

    let mut swapped = Board::new();
    swapped.make(1, 3, 3);
    swapped.make(0, 4, 4);
    assert_ne!(a.hash(), swapped.hash());
}
