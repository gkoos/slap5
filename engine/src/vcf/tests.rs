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
