use crate::Board;
use crate::bitboard::{Bits, SIZE};

/// Player 0 is the engine, player 1 the opponent. Coordinates are protocol (x = column, y = row).
pub struct Game {
    board: Board,
    moves: Vec<(usize, usize, usize)>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        Game { board: Board::new(), moves: Vec::new() }
    }

    pub fn moves(&self) -> &[(usize, usize, usize)] {
        &self.moves
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn occupied(&self, x: usize, y: usize) -> bool {
        let i = y * crate::bitboard::STRIDE + x;
        self.board.stones[0].get(i) || self.board.stones[1].get(i)
    }

    pub fn play(&mut self, player: usize, x: usize, y: usize) -> Result<(), &'static str> {
        if x >= SIZE || y >= SIZE {
            return Err("coordinates out of range");
        }
        if self.occupied(x, y) {
            return Err("cell already occupied");
        }
        self.board.make(player, y, x);
        self.moves.push((player, x, y));
        Ok(())
    }

    /// Removes the stone at (x, y); returns false if there is none.
    pub fn undo(&mut self, x: usize, y: usize) -> bool {
        let Some(pos) = self.moves.iter().position(|&(_, mx, my)| mx == x && my == y) else {
            return false;
        };
        let (player, _, _) = self.moves.remove(pos);
        self.board.unmake(player, y, x);
        true
    }

    pub fn wins(&self, player: usize) -> bool {
        self.board.wins(player)
    }

    pub fn hash(&self) -> u64 {
        self.board.hash()
    }

    pub fn is_full(&self) -> bool {
        self.moves.len() == SIZE * SIZE
    }
}

#[cfg(test)]
mod tests;
