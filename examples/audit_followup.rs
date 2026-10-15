//! Replay second-audit forcing wins with the current engine and optional ablations.
//! cargo run --release --example audit_followup -- 1000 [no-double|no-threes|...]
//! `defend` removes high5's last move and searches the preceding defensive decision.
use high5::search::{Config, Limits, Searcher};
use high5::{Board, eval};
use std::time::Duration;

fn main() {
    let mut config = Config::default();
    let mut ms = 1000;
    let mut defend = false;
    let mut max_depth = 100;
    for arg in std::env::args().skip(1) {
        if let Ok(n) = arg.parse() {
            ms = n;
        } else if arg == "defend" {
            defend = true;
        } else if let Some(n) = arg.strip_prefix("depth=") {
            max_depth = n.parse().unwrap();
        } else if let Some(n) = arg.strip_prefix("cap=") {
            config.free_ply_limit = n.parse().unwrap();
        } else if !config.apply(&arg) {
            panic!("unknown argument {arg}");
        }
    }
    println!("{config:?} defend={defend} max_depth={max_depth}");
    for line in include_str!("../data/audit_followup.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (name, rest) = line.split_once(" us=1 stones=").unwrap();
        let mut stones: Vec<(usize, usize, usize)> = rest
            .split_whitespace()
            .map(|s| {
                let (p, xy) = s.split_once(':').unwrap();
                let (x, y) = xy.split_once(',').unwrap();
                (p.parse().unwrap(), x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        if defend {
            stones.pop();
        }
        let mut board = Board::new();
        for (p, x, y) in stones {
            board.make(p, y, x);
        }
        let side = if defend { 0 } else { 1 };
        let mut searcher = Searcher::new();
        searcher.config = config;
        let result = searcher
            .search(
                &board,
                side,
                Limits {
                    time: Duration::from_millis(ms),
                    max_depth,
                },
            )
            .unwrap();
        println!(
            "{name} side={side} static={} move=({}, {}) score={} depth={} ms={} mate={:?} {}",
            eval::evaluate(&board, side),
            result.x,
            result.y,
            result.score,
            result.depth,
            result.elapsed.as_millis(),
            result.mate_verified,
            result.stats
        );
    }
}
