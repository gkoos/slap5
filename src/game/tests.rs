use super::*;

#[test]
fn play_and_undo() {
    let mut g = Game::new();
    g.play(0, 3, 4).unwrap();
    assert!(g.occupied(3, 4));
    assert!(g.play(1, 3, 4).is_err());
    assert!(g.play(1, 15, 0).is_err());
    assert!(g.undo(3, 4));
    assert!(!g.occupied(3, 4));
    assert!(!g.undo(3, 4));
}

#[test]
fn undo_restores_hash() {
    let mut g = Game::new();
    g.play(0, 3, 4).unwrap();
    let before = g.hash();
    g.play(1, 8, 8).unwrap();
    g.play(0, 9, 9).unwrap();
    g.undo(8, 8);
    g.undo(9, 9);
    assert_eq!(g.hash(), before);
    assert_ne!(before, 0);
}

#[test]
fn detects_five() {
    let mut g = Game::new();
    for x in 2..7 {
        g.play(1, x, 9).unwrap();
    }
    assert!(g.wins(1));
    assert!(!g.wins(0));
}
