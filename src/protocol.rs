use crate::bitboard::SIZE;
use crate::game::Game;
use crate::search::{Limits, Searcher};
use std::time::Duration;

/// Gomocup brain protocol: feed it one line at a time, it returns the reply lines.
/// A reply to a move command ends with the move; lines before it are DEBUG output.
#[derive(Default)]
pub struct Engine {
    game: Option<Game>,
    board_lines: Option<Vec<String>>,
    finished: bool,
    searcher: Searcher,
    timeout_turn: Option<u64>,
    time_left: Option<i64>,
}

fn parse_xy(s: &str) -> Option<(usize, usize)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn err(msg: &str) -> Vec<String> {
    vec![format!("ERROR {msg}")]
}

// Placeholder until proper time management: a tenth of the remaining match time at most, 50 ms spare.
fn budget(timeout_turn: Option<u64>, time_left: Option<i64>) -> Duration {
    let mut ms = match timeout_turn {
        Some(0) => 50,
        Some(t) => t as i64,
        None => 1000,
    };
    if let Some(left) = time_left {
        ms = ms.min(left / 10);
    }
    Duration::from_millis((ms - 50).max(20) as u64)
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn handle(&mut self, line: &str) -> Vec<String> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        if self.board_lines.is_some() {
            return self.board_line(line);
        }
        let (cmd, arg) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let arg = arg.trim();
        match cmd.to_ascii_uppercase().as_str() {
            "START" => match arg.parse::<usize>() {
                Ok(n) if n == SIZE => {
                    self.game = Some(Game::new());
                    vec!["OK".into()]
                }
                _ => err("unsupported board size, only 15 is supported"),
            },
            "RECTSTART" => err("rectangular boards are not supported"),
            "RESTART" => {
                self.game = Some(Game::new());
                vec!["OK".into()]
            }
            "INFO" => {
                self.info(arg);
                vec![]
            }
            "END" => {
                self.finished = true;
                vec![]
            }
            "ABOUT" => vec![format!("name=\"high5\", version=\"{}\"", env!("CARGO_PKG_VERSION"))],
            "BEGIN" => self.reply_move(),
            "TURN" => self.turn(arg),
            "BOARD" => {
                self.board_lines = Some(Vec::new());
                vec![]
            }
            "TAKEBACK" => self.takeback(arg),
            _ => vec![format!("UNKNOWN {cmd}")],
        }
    }

    fn info(&mut self, arg: &str) {
        let (key, value) = arg.split_once(char::is_whitespace).unwrap_or((arg, ""));
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "timeout_turn" => self.timeout_turn = value.parse().ok(),
            "time_left" => self.time_left = value.parse().ok(),
            _ => {}
        }
    }

    fn reply_move(&mut self) -> Vec<String> {
        let Some(game) = self.game.as_mut() else {
            return err("not started");
        };
        let limits = Limits { time: budget(self.timeout_turn, self.time_left), max_depth: 100 };
        let Some(r) = self.searcher.search(game.board(), 0, limits) else {
            return err("board is full");
        };
        game.play(0, r.x, r.y).expect("search returns an empty cell");
        vec![
            format!(
                "DEBUG depth {} score {} mate {} {} time {}ms",
                r.depth,
                r.score,
                match r.mate_verified {
                    Some(true) => "verified",
                    Some(false) => "unverified",
                    None => "none",
                },
                r.stats,
                r.elapsed.as_millis()
            ),
            format!("{},{}", r.x, r.y),
        ]
    }

    fn turn(&mut self, arg: &str) -> Vec<String> {
        let Some(game) = self.game.as_mut() else {
            return err("not started");
        };
        let Some((x, y)) = parse_xy(arg) else {
            return err("invalid coordinates");
        };
        if let Err(e) = game.play(1, x, y) {
            return err(e);
        }
        self.reply_move()
    }

    fn takeback(&mut self, arg: &str) -> Vec<String> {
        let Some(game) = self.game.as_mut() else {
            return err("not started");
        };
        match parse_xy(arg) {
            Some((x, y)) if game.undo(x, y) => vec!["OK".into()],
            _ => err("no stone at these coordinates"),
        }
    }

    fn board_line(&mut self, line: &str) -> Vec<String> {
        if !line.eq_ignore_ascii_case("DONE") {
            self.board_lines.as_mut().expect("collecting").push(line.to_string());
            return vec![];
        }
        let lines = self.board_lines.take().expect("collecting");
        if self.game.is_none() {
            return err("not started");
        }
        let mut game = Game::new();
        for l in &lines {
            let mut parts = l.splitn(3, ',');
            let (Some(x), Some(y), Some(f)) = (parts.next(), parts.next(), parts.next()) else {
                return err("invalid BOARD line");
            };
            let (Ok(x), Ok(y)) = (x.trim().parse(), y.trim().parse()) else {
                return err("invalid BOARD line");
            };
            let player = match f.trim() {
                "1" => 0,
                "2" => 1,
                _ => continue,
            };
            if let Err(e) = game.play(player, x, y) {
                return err(e);
            }
        }
        self.game = Some(game);
        self.reply_move()
    }
}

#[cfg(test)]
mod tests;
