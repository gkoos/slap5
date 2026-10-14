//! Runs the engine on the positions in data/last_chance.txt and counts how often it picks a saving move.
//! Usage: cargo run --release --example suite [-- <ms per position> [switch ...]]
//! Switches: see `Config::apply` (no-tt, no-threes, no-leaf, leaf=N, no-selective).

use high5::Board;
use high5::search::{Config, Limits, Searcher, Width};
use std::time::Duration;

fn xy(s: &str) -> (usize, usize) {
    let (x, y) = s.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

fn main() {
    let mut config = Config::default();
    let mut ms = 1000;
    for arg in std::env::args().skip(1) {
        if let Ok(n) = arg.parse() {
            ms = n;
        } else if !config.apply(&arg) {
            panic!("unknown argument {arg}");
        }
    }
    let mut total = 0;
    let mut saved = 0;
    let mut depths = Vec::new();
    let mut width = [[[[Width::default(); 9]; 2]; 2]; 2];
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
        searcher.config = config;
        let r = searcher.search(&board, us, Limits { time: Duration::from_millis(ms), max_depth: 100 }).unwrap();
        let ok = saving.contains(&(r.x, r.y));
        total += 1;
        saved += ok as u32;
        depths.push(r.depth);
        for (f, t, c, d) in (0..2).flat_map(|f| (0..2).flat_map(move |t| (0..2).flat_map(move |c| (0..9).map(move |d| (f, t, c, d))))) {
            let (w, s) = (&mut width[f][t][c][d], r.stats.width[f][t][c][d]);
            w.nodes += s.nodes;
            w.candidates += s.candidates;
            w.searched += s.searched;
        }
        println!("{name:<32} depth {} -> ({},{}) {}  {}  time {}ms", r.depth, r.x, r.y, if ok { "saving" } else if (r.x, r.y) == played { "same losing move as in the game" } else { "not saving" }, r.stats, r.elapsed.as_millis());
    }
    println!("\n{saved}/{total} saving moves at {ms} ms per position (depths {}..{}), {config:?}", depths.iter().min().unwrap(), depths.iter().max().unwrap());

    println!("\nInterior nodes: forced / tactical (a four or open three on the board) / cut, by remaining depth");
    println!("{:<8} {:<9} {:<4} {:>5} {:>12} {:>12} {:>12}", "forced", "tactical", "cut", "depth", "nodes", "candidates", "searched");
    for (f, t, c, d) in (0..2).flat_map(|f| (0..2).flat_map(move |t| (0..2).flat_map(move |c| (0..9).map(move |d| (f, t, c, d))))) {
        let w = width[f][t][c][d];
        if w.nodes > 0 {
            let avg = |x: u64| x as f64 / w.nodes as f64;
            println!("{:<8} {:<9} {:<4} {:>5} {:>12} {:>12.1} {:>12.2}", f == 1, t == 1, c == 1, d, w.nodes, avg(w.candidates), avg(w.searched));
        }
    }
}
