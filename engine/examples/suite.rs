//! Runs the engine on the positions in examples/last_chance.txt and counts how often it picks a saving move.
//! Usage: cargo run --release --example suite [-- <ms per position> [switch ...]]
//! Switches: see `Config::apply` (no-tt, no-threes, no-leaf, leaf=N, no-selective, no-limit, no-reductions).

use slap5::Board;
use slap5::search::{Config, Limits, Searcher, Width};
use std::time::Duration;

fn xy(s: &str) -> (usize, usize) {
    let (x, y) = s.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

fn main() {
    let mut config = Config::default();
    let mut ms = 1000;
    let mut tt_bits = None;
    let mut only = None;
    let mut after = None;
    let mut depth = 100;
    for arg in std::env::args().skip(1) {
        if let Ok(n) = arg.parse() {
            ms = n;
        } else if let Some(b) = arg.strip_prefix("tt-bits=") {
            tt_bits = Some(b.parse::<u32>().expect("tt-bits=<n>"));
        } else if let Some(name) = arg.strip_prefix("only=") {
            only = Some(name.to_owned());
        } else if let Some(m) = arg.strip_prefix("after=") {
            after = Some(xy(m));
        } else if let Some(d) = arg.strip_prefix("depth=") {
            depth = d.parse().expect("depth=<n>");
        } else if !config.apply(&arg) {
            panic!("unknown argument {arg}");
        }
    }
    let mut total = 0;
    let mut saved = 0;
    let mut depths = Vec::new();
    let mut width = [[[[Width::default(); 9]; 2]; 2]; 2];
    for line in include_str!("last_chance.txt").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap();
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
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
        let mut searcher = tt_bits.map_or_else(Searcher::new, Searcher::with_table_bits);
        searcher.config = config;
        let limits = Limits { time: Duration::from_millis(ms), max_depth: depth };
        // `after=x,y`: our move is played first and the search is the opponent's reply, from its side.
        if let Some((x, y)) = after {
            board.make(us, y, x);
            us = 1 - us;
        }
        let r = if only.is_some() {
            let started = std::time::Instant::now();
            let mut report = |depth: i32, x: usize, y: usize, score: i32| {
                let verdict = if saving.contains(&(x, y)) { "saving" } else if (x, y) == played { "the game's losing move" } else { "not saving" };
                println!("  depth {depth:>2} at {:>5} ms -> ({x},{y}) score {score:>6} {verdict}", started.elapsed().as_millis());
            };
            searcher.search_observing(&board, us, limits, &mut report).unwrap()
        } else {
            searcher.search(&board, us, limits).unwrap()
        };
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
        println!("{name:<32} depth {} -> ({},{}) {} score {} {}  time {}ms", r.depth, r.x, r.y, if ok { "saving" } else if (r.x, r.y) == played { "same losing move as in the game" } else { "not saving" }, r.score, r.stats, r.elapsed.as_millis());
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
