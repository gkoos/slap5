use super::*;
use crate::bitboard::SIZE;
use crate::Words;

type B = Board<Words>;

fn with(stones: &[(usize, usize, usize)]) -> B {
    let mut b = B::new();
    for &(p, r, c) in stones {
        b.make(p, r, c);
    }
    b
}

#[test]
fn empty_board_scores_only_tempo() {
    // The shape part of an empty board is zero by definition, so what is left is the side-to-move term -
    // which is opposite for the two colours, and the reason this test no longer expects zero.
    let black = evaluate(&B::new(), 0);
    let white = evaluate(&B::new(), 1);
    assert_eq!(black, -white, "{black} {white}");
    assert_eq!(black, TEMPO, "{black}");
}

#[test]
fn longer_open_lines_score_higher() {
    let two = evaluate(&with(&[(0, 7, 6), (0, 7, 7)]), 0);
    let three = evaluate(&with(&[(0, 7, 6), (0, 7, 7), (0, 7, 8)]), 0);
    let four = evaluate(&with(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (0, 7, 9)]), 0);
    assert!(0 < two && two < three && three < four, "{two} {three} {four}");
}

#[test]
fn a_blocked_line_scores_lower_than_an_open_one() {
    let open = evaluate(&with(&[(0, 7, 6), (0, 7, 7), (0, 7, 8)]), 0);
    let blocked = evaluate(&with(&[(0, 7, 6), (0, 7, 7), (0, 7, 8), (1, 7, 5)]), 0);
    assert!(blocked < open, "{blocked} {open}");
}

#[test]
fn opponent_threats_count_against_the_side_to_move() {
    let b = with(&[(1, 7, 6), (1, 7, 7), (1, 7, 8)]);
    assert!(evaluate(&b, 0) < 0);
    assert_eq!(evaluate(&b, 0), -evaluate(&b, 1));
}

// The eight symmetries of the square board must not change the score.
fn transform(k: usize, r: usize, c: usize) -> (usize, usize) {
    let (mut r, mut c) = (r, c);
    if k & 1 != 0 {
        (r, c) = (c, r);
    }
    if k & 2 != 0 {
        r = SIZE - 1 - r;
    }
    if k & 4 != 0 {
        c = SIZE - 1 - c;
    }
    (r, c)
}

#[test]
fn score_is_invariant_under_board_symmetries() {
    let mut s = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut nonzero = 0;
    for _ in 0..500 {
        let (r0, c0) = ((next() % 6) as usize, (next() % 6) as usize);
        let mut stones = Vec::new();
        while stones.len() < 25 {
            let r = r0 + (next() % 9) as usize;
            let c = c0 + (next() % 9) as usize;
            if !stones.iter().any(|&(_, sr, sc)| (sr, sc) == (r, c)) {
                stones.push((stones.len() % 2, r, c));
            }
        }
        let base = evaluate(&with(&stones), 0);
        nonzero += (base != 0) as u32;
        for k in 1..8 {
            let moved: Vec<_> = stones
                .iter()
                .map(|&(p, r, c)| {
                    let (r, c) = transform(k, r, c);
                    (p, r, c)
                })
                .collect();
            assert_eq!(evaluate(&with(&moved), 0), base, "symmetry {k} of {stones:?}");
        }
    }
    assert!(nonzero > 100, "only {nonzero} positions had a non-zero score");
}
