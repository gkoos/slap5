use crate::bitboard::SIZE;
use crate::game::Game;

/// Placeholder: the empty cell nearest the opponent's last stone (the centre on an empty board).
pub fn choose(game: &Game) -> Option<(usize, usize)> {
    let centre = (SIZE / 2, SIZE / 2);
    let anchor = game
        .moves()
        .iter()
        .rev()
        .find(|m| m.0 == 1)
        .map_or(centre, |m| (m.1, m.2));
    let dist = |a: (usize, usize), b: (usize, usize)| (a.0.abs_diff(b.0).max(a.1.abs_diff(b.1)), a.0.abs_diff(b.0) + a.1.abs_diff(b.1));
    (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
        .filter(|&(x, y)| !game.occupied(x, y))
        .min_by_key(|&p| (dist(p, anchor), dist(p, centre)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_on_empty_board() {
        assert_eq!(choose(&Game::new()), Some((7, 7)));
    }

    #[test]
    fn plays_next_to_opponent() {
        let mut g = Game::new();
        g.play(1, 2, 2).unwrap();
        let (x, y) = choose(&g).unwrap();
        assert!(x.abs_diff(2).max(y.abs_diff(2)) == 1);
    }

    #[test]
    fn none_when_full() {
        let mut g = Game::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                g.play(0, x, y).unwrap();
            }
        }
        assert_eq!(choose(&g), None);
    }
}
