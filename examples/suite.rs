//! Runs the engine on the positions in data/last_chance.txt and counts how often it picks a saving move.
//! Usage: cargo run --release --example suite [-- <ms per position>]

use high5::Board;
use high5::search::{Limits, Searcher};
use std::time::Duration;

fn xy(s: &str) -> (usize, usize) {
    let (x, y) = s.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

fn main() {
    let ms: u64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1000);
    let mut total = 0;
    let mut saved = 0;
    let mut depths = Vec::new();
    for line in include_str!("../data/last_chance.txt").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap();
        let (mut us, mut played, mut stones, mut saving) = (0, (0, 0), Vec::new(), Vec::new());
        let mut field = "";
        for tok in parts {
            if let Some(v) = tok.strip_prefix("us=") {
                us = v.parse().unwrap();
                field = "us";
            } else if let Some(v) = tok.strip_prefix("stones=") {
                stones.push(v);
                field = "stones";
            } else if let Some(v) = tok.strip_prefix("played=") {
                played = xy(v);
                field = "played";
            } else if let Some(v) = tok.strip_prefix("saving=") {
                saving.push(xy(v));
                field = "saving";
            } else if field == "stones" {
                stones.push(tok);
            } else if field == "saving" {
                saving.push(xy(tok));
            }
        }
        let mut board = Board::new();
        for s in stones {
            let (p, c) = s.split_once(':').unwrap();
            let (x, y) = xy(c);
            board.make(p.parse().unwrap(), y, x);
        }
        assert!(!saving.contains(&played), "{name}: the played move is listed as saving");
        let mut searcher = Searcher::new();
        let r = searcher.search(&board, us, Limits { time: Duration::from_millis(ms), max_depth: 100 }).unwrap();
        let ok = saving.contains(&(r.x, r.y));
        total += 1;
        saved += ok as u32;
        depths.push(r.depth);
        println!("{name:<32} depth {} -> ({},{}) {}", r.depth, r.x, r.y, if ok { "saving" } else if (r.x, r.y) == played { "same losing move as in the game" } else { "not saving" });
    }
    println!("\n{saved}/{total} saving moves at {ms} ms per position (depths {}..{})", depths.iter().min().unwrap(), depths.iter().max().unwrap());
}
