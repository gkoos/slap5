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
            format!("DEBUG depth {} score {} nodes {} time {}ms", r.depth, r.score, r.nodes, r.elapsed.as_millis()),
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
mod tests {
    use super::*;

    fn run(e: &mut Engine, lines: &[&str]) -> Vec<String> {
        lines.iter().flat_map(|l| e.handle(l)).collect()
    }

    fn last(lines: &[String]) -> &str {
        lines.last().expect("a reply")
    }

    fn started() -> Engine {
        let mut e = Engine::new();
        assert_eq!(e.handle("START 15"), ["OK"]);
        e.handle("INFO timeout_turn 100");
        e
    }

    #[test]
    fn start_sizes() {
        let mut e = Engine::new();
        assert_eq!(e.handle("START 15"), ["OK"]);
        assert!(e.handle("START 20")[0].starts_with("ERROR"));
        assert!(e.handle("RECTSTART 15,15")[0].starts_with("ERROR"));
    }

    #[test]
    fn begin_plays_centre() {
        let mut e = started();
        assert_eq!(last(&e.handle("BEGIN")), "7,7");
    }

    #[test]
    fn turn_returns_legal_move() {
        let mut e = started();
        let r = e.handle("TURN 7,7");
        assert!(r[..r.len() - 1].iter().all(|l| l.starts_with("DEBUG ")));
        let (x, y) = parse_xy(last(&r)).unwrap();
        assert!(x < 15 && y < 15 && (x, y) != (7, 7));
    }

    #[test]
    fn rejects_bad_turns() {
        let mut e = started();
        e.handle("TURN 7,7");
        assert!(e.handle("TURN 7,7")[0].starts_with("ERROR"));
        assert!(e.handle("TURN 15,0")[0].starts_with("ERROR"));
        assert!(e.handle("TURN a,b")[0].starts_with("ERROR"));
    }

    #[test]
    fn commands_before_start_fail() {
        let mut e = Engine::new();
        assert!(e.handle("BEGIN")[0].starts_with("ERROR"));
        assert!(e.handle("TURN 1,1")[0].starts_with("ERROR"));
    }

    #[test]
    fn board_command() {
        let mut e = started();
        let r = run(&mut e, &["BOARD", "10,10,1", "10,11,2", "11,11,1", "9,10,2", "DONE"]);
        let (x, y) = parse_xy(last(&r)).unwrap();
        assert!(![(10, 10), (10, 11), (11, 11), (9, 10)].contains(&(x, y)));
    }

    #[test]
    fn board_with_overlapping_stones_errors() {
        let mut e = started();
        let r = run(&mut e, &["BOARD", "1,1,1", "1,1,2", "DONE"]);
        assert!(r[0].starts_with("ERROR"));
    }

    #[test]
    fn takeback_frees_cell() {
        let mut e = started();
        e.handle("TURN 3,3");
        assert_eq!(e.handle("TAKEBACK 3,3"), ["OK"]);
        assert!(e.handle("TAKEBACK 3,3")[0].starts_with("ERROR"));
        assert!(parse_xy(last(&e.handle("TURN 3,3"))).is_some());
    }

    #[test]
    fn restart_clears_board() {
        let mut e = started();
        e.handle("TURN 7,7");
        assert_eq!(e.handle("RESTART"), ["OK"]);
        assert_eq!(last(&e.handle("BEGIN")), "7,7");
    }

    #[test]
    fn plays_the_winning_move() {
        let mut e = started();
        let r = run(&mut e, &["BOARD", "3,7,1", "4,7,1", "5,7,1", "6,7,1", "10,10,2", "11,10,2", "DONE"]);
        assert!(["2,7", "7,7"].contains(&last(&r)), "{r:?}");
    }

    #[test]
    fn time_budget() {
        assert_eq!(budget(Some(1000), None), Duration::from_millis(950));
        assert_eq!(budget(None, None), Duration::from_millis(950));
        assert_eq!(budget(Some(0), None), Duration::from_millis(20));
        assert_eq!(budget(Some(1000), Some(2000)), Duration::from_millis(150));
        assert_eq!(budget(Some(1000), Some(-500)), Duration::from_millis(20));
        assert_eq!(budget(Some(1000), Some(2_147_483_647)), Duration::from_millis(950));
    }

    #[test]
    fn misc_commands() {
        let mut e = started();
        assert!(e.handle("INFO timeout_turn 1000").is_empty());
        assert!(e.handle("ABOUT")[0].starts_with("name=\"high5\""));
        assert_eq!(e.handle("SWAP2BOARD"), ["UNKNOWN SWAP2BOARD"]);
        assert!(!e.finished());
        assert!(e.handle("END").is_empty());
        assert!(e.finished());
    }
}
