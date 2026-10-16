//! Measures how well the static evaluation predicts the search it feeds: for each position in the corpus,
//! the static score at the root against the score of a fixed-depth search, plus agreement statistics and the
//! worst residuals. This is the headroom test for evaluation work (`docs/roadmap.md` item 3): if the two
//! agree closely there is little to gain from more features, and if the residuals cluster, the clusters are
//! the feature list, ranked by evidence rather than taste.
//! Usage: cargo run --release --example eval_probe [-- <depth>]

use high5::Board;
use high5::bitboard::Bits;
use high5::search::{Config, Limits, Searcher};
use std::time::Duration;

fn xy(s: &str) -> (usize, usize) {
    let (x, y) = s.split_once(',').expect("x,y");
    (x.parse().unwrap(), y.parse().unwrap())
}

fn main() {
    let depth = std::env::args().skip(1).find_map(|a| a.parse::<i32>().ok()).unwrap_or(6);
    let mut rows: Vec<(String, usize, i32, i32, i32, usize)> = Vec::new();
    for line in include_str!("last_chance.txt").lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let mut parts = line.split_whitespace();
        let name = parts.next().unwrap().to_owned();
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
        let static_eval = high5::eval::evaluate(&board, us);
        let mut searcher = Searcher::new();
        searcher.config = Config::default();
        let r = searcher
            .search(&board, us, Limits { time: Duration::from_secs(3600), max_depth: depth })
            .unwrap();
        rows.push((name, us, static_eval, r.score, r.score - static_eval, (board.stones[0].count_ones() + board.stones[1].count_ones()) as usize));
    }

    println!("{:<32} {:>3} {:>10} {:>10} {:>10}", "position", "us", "static", "search", "residual");
    let mut sorted = rows.clone();
    sorted.sort_by_key(|r| -r.4.abs());
    for r in sorted.iter().take(8) {
        println!("{:<32} {:>3} {:>10} {:>10} {:>10}", r.0, r.1, r.2, r.3, r.4);
    }

    let stats = |set: &[(String, usize, i32, i32, i32, usize)]| {
        let n = set.len() as f64;
        if n == 0.0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let (mx, my) = (
            set.iter().map(|r| r.2 as f64).sum::<f64>() / n,
            set.iter().map(|r| r.3 as f64).sum::<f64>() / n,
        );
        let (mut sxy, mut sxx, mut syy, mut mse) = (0.0, 0.0, 0.0, 0.0);
        for r in set {
            let (dx, dy) = (r.2 as f64 - mx, r.3 as f64 - my);
            sxy += dx * dy;
            sxx += dx * dx;
            syy += dy * dy;
            mse += (r.4 as f64) * (r.4 as f64);
        }
        let r_ = if sxx > 0.0 && syy > 0.0 { sxy / (sxx.sqrt() * syy.sqrt()) } else { f64::NAN };
        (r_, (mse / n).sqrt(), set.iter().map(|r| r.4 as f64).sum::<f64>() / n, set.iter().map(|r| r.4.abs()).max().unwrap_or(0) as f64)
    };

    let quiet = rows.iter().filter(|r| r.3.abs() < 29_000).cloned().collect::<Vec<_>>();
    for (label, set) in [
        ("all", rows.as_slice()),
        ("quiet (decided removed)", quiet.as_slice()),
        ("quiet early (<20)", quiet.iter().filter(|r| r.5 < 20).cloned().collect::<Vec<_>>().as_slice()),
        ("quiet late (>=20)", quiet.iter().filter(|r| r.5 >= 20).cloned().collect::<Vec<_>>().as_slice()),
    ] {
        let (r_, sd, mean, worst) = stats(set);
        println!("\n{label:<24} n={:>3}  r={r_:.3}  sd(residual)={sd:.0}  mean={mean:.0}  max|residual|={worst:.0}", set.len());
    }
    println!("\ndecided positions, where the search score is a mate (a count model has no vocabulary for it): {}", rows.len() - quiet.len());
    println!("\ndepth {depth}, corpus {}.", rows.len());
}
