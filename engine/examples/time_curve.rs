//! One long search per position, printing every completed iteration with its completion time, so
//! `engine/scripts/time_curve.py analyse` can read off the move any shorter budget would have played.
//! Usage: cargo run --release --example time_curve -- <positions file> <ms per position> <output file>
//! Positions file: `id phase stones side x,y x,y ...`, moves 0-based in play order, black first.
//! Output: `id iter <depth> <ms> <x,y> <score>` per iteration, then `id final <ms> <x,y> <score> <depth>`,
//! each line flushed as it is written so a running measurement can be read.

use slap5::Board;
use slap5::search::{Limits, Searcher};
use std::io::Write;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = std::fs::read_to_string(&args[0]).expect("positions file");
    let ms: u64 = args[1].parse().expect("ms");
    let mut out = std::fs::File::create(&args[2]).expect("output file");
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let p: Vec<&str> = line.split_whitespace().collect();
        let (id, side): (usize, usize) = (p[0].parse().unwrap(), p[3].parse().unwrap());
        let mut board = Board::new();
        for (k, m) in p[4..].iter().enumerate() {
            let (x, y) = m.split_once(',').unwrap();
            board.make(k % 2, y.parse().unwrap(), x.parse().unwrap());
        }
        let mut searcher = Searcher::new();
        let started = Instant::now();
        let mut lines = Vec::new();
        let mut report = |depth: i32, x: usize, y: usize, score: i32| {
            lines.push(format!("{id} iter {depth} {} {x},{y} {score}", started.elapsed().as_millis()));
        };
        let limits = Limits { time: Duration::from_millis(ms), max_depth: 100 };
        let r = searcher.search_observing(&board, side, limits, &mut report).unwrap();
        lines.push(format!("{id} final {} {},{} {} {}", r.elapsed.as_millis(), r.x, r.y, r.score, r.depth));
        // Written after the search, so file I/O never lands inside the timed iterations.
        for l in lines {
            writeln!(out, "{l}").expect("write");
        }
        out.flush().expect("flush");
        eprintln!("{id} done");
    }
}
