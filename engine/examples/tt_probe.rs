//! Nodes to reach a fixed depth on the suite positions (examples/last_chance.txt), for several table sizes.
//! No clock is involved, so the counts are deterministic: a larger table can only show up as fewer nodes to
//! the same depth. At a depth shallow enough that no table fills, every size must give the same counts.
//! Usage: cargo run --release --example tt_probe [-- <depth> <bits> <bits> ...]   (default: 8 21 25)

use slap5::Board;
use slap5::search::{Limits, Searcher};
use std::time::{Duration, Instant};

fn xy(s: &str) -> (usize, usize) {
    let (x, y) = s.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

fn main() {
    let args: Vec<u32> = std::env::args().skip(1).map(|a| a.parse().expect("numbers")).collect();
    let depth = args.first().copied().unwrap_or(8) as i32;
    let sizes: Vec<u32> = if args.len() > 1 { args[1..].to_vec() } else { vec![21, 25] };
    let mut totals = vec![(0u64, Duration::ZERO); sizes.len()];
    print!("{:<32}", "position");
    for b in &sizes {
        print!(" {:>14} {:>8}", format!("nodes 2^{b}"), "ms");
    }
    println!();
    for line in include_str!("last_chance.txt").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap();
        let (mut us, mut stones) = (0usize, Vec::new());
        let mut field = "";
        for tok in parts {
            if let Some(v) = tok.strip_prefix("us=") {
                us = v.parse().unwrap();
                field = "us";
            } else if let Some(v) = tok.strip_prefix("stones=") {
                stones.push(v);
                field = "stones";
            } else if tok.contains('=') {
                field = "";
            } else if field == "stones" {
                stones.push(tok);
            }
        }
        let mut board = Board::new();
        for s in stones {
            let (p, c) = s.split_once(':').unwrap();
            let (x, y) = xy(c);
            board.make(p.parse().unwrap(), y, x);
        }
        print!("{name:<32}");
        for (i, &bits) in sizes.iter().enumerate() {
            let mut searcher = Searcher::with_table_bits(bits);
            let started = Instant::now();
            let r = searcher.search(&board, us, Limits { time: Duration::from_secs(3600), max_depth: depth }).unwrap();
            let took = started.elapsed();
            totals[i].0 += r.stats.nodes;
            totals[i].1 += took;
            print!(" {:>14} {:>8}", r.stats.nodes, took.as_millis());
        }
        println!();
    }
    print!("{:<32}", format!("total, depth {depth}"));
    for (nodes, took) in &totals {
        print!(" {:>14} {:>8}", nodes, took.as_millis());
    }
    println!();
}
