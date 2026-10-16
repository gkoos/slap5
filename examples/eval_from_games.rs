//! Builds an evaluation corpus from the recorded games and measures static-vs-search agreement on it.
//!
//! The suite's positions are 21 and all lost (`examples/eval_probe.rs` shows why that matters: six of them
//! are mate scores a count model cannot express), so tuning needs positions from real games at every phase.
//! Every match writes its moves to `logs/<run>/*.psq` as `x,y,player` lines, and both engines' games are in
//! there, so the corpus is ours by construction and costs nothing to collect.
//!
//! Usage: cargo run --release --example eval_from_games [-- <stride> <max> <depth> <dir>]

use high5::Board;
use high5::bitboard::Bits;
use high5::search::{Config, Limits, Searcher};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stride: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(4);
    let max: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(150);
    let depth: i32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(6);
    let dir = args.get(3).cloned().unwrap_or_else(|| "logs".to_string());
    // Fifth argument: keep only positions the static model already reads as deeply negative, e.g. 1500 for
    // static <= -1500. The tail of the residual distribution is what item 3 has to explain, and it is the one
    // subset worth relabelling at a depth the whole corpus could never afford, so the filter exists to make
    // that relabelling cheap. Zero (the default) keeps everything.
    let min_pessimism: i32 = args.get(4).and_then(|a| a.parse().ok()).unwrap_or(0);

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("corpus directory")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().filter_map(|e| e.ok()).map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "psq").unwrap_or(false))
        .collect();
    files.sort();

    // A per-file quota, not a global one. There are 1,699 records under logs/ and a global quota fills itself
    // from the few runs that sort first, which is a handful of games wearing the name of a corpus. Spreading
    // the sample over every record is what makes a fit/hold-out split mean anything, and the global cap still
    // bounds the work when the corpus is small.
    let per_file = (max / files.len().max(1)).max(1);
    let mut positions: Vec<(String, Board, usize)> = Vec::new();
    for (file_index, path) in files.iter().enumerate() {
        if positions.len() >= max {
            break;
        }
        let before = positions.len();
        // Spread the sample across the game, not just the opening. Without this every file contributes its
        // first qualifying position, which put 982 of 987 positions under ten stones; with it, file k
        // contributes a position near ply 8 + 8*(k mod 24), so the sample walks through the middle game and
        // the endgame too, and the phase bands fill instead of collapsing into the first one.
        let offset = 8 * (file_index % 24);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut board = Board::new();
        let mut last;
        let mut ply = 0usize;
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        for line in text.lines() {
            let parts: Vec<&str> = line.trim().split(',').collect();
            if parts.len() < 2 {
                continue;
            }
            let (Ok(x), Ok(y)) = (parts[0].trim().parse::<usize>(), parts[1].trim().parse::<usize>()) else {
                continue;
            };
            if x >= 15 || y >= 15 {
                continue;
            }
            // The third field is the move time in milliseconds, not the player (`8,7,459`), and the first
            // few lines are Piskvork's opening stones with time 0. Players therefore alternate from the first
            // recorded stone; the run directory name says who that was.
            let p = ply % 2;
            board.make(p, y, x);
            last = p;
            ply += 1;
            // Quiet positions only, and past the opening: a position where either side can complete five is
            // decided, and the count model cannot express it any better than the search can.
            if ply % stride == 0
                && ply > 5
                && positions.len() < max
                && board.five_cells(0).is_zero()
                && board.five_cells(1).is_zero()
            {
                if positions.len() - before < per_file
                    && ply >= 8 + offset
                    && (min_pessimism <= 0 || high5::eval::evaluate(&board, 1 - last) <= -min_pessimism)
                {
                    positions.push((format!("{name}#{ply}"), board, 1 - last));
                }
            }
        }
    }

    // Labelling is one fixed-depth search per position and each position is independent, so the work goes
    // across threads: same results, wall clock divided by the core count. One searcher per thread, not one
    // per position - a searcher allocates a 32 MB table, and the earlier single-threaded version paid that
    // per position, which is a large part of why a 300-position run did not finish in fourteen minutes.
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
    // Positions cost wildly different amounts (a late position runs into the forced-chain explosion that
    // dominates our tree, an early one is trivial), so equal-sized chunks leave most threads idle behind one
    // straggler. A shared cursor hands out positions in cost order as threads free up, which is what makes
    // the wall clock track the total work instead of the worst chunk.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let positions = &positions;
    let parts: Vec<Vec<(String, i32, i32, i32, usize)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads.min(positions.len().max(1)))
            .map(|_| {
                let (next, done) = (&next, &done);
                scope.spawn(move || {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((name, board, us)) = positions.get(i) else { break };
                        // A fresh searcher per position, not per thread: the transposition table otherwise
                        // carries over, so a label depends on what the same thread happened to search before
                        // (one board scored 1212, 1157 and 975 at the same depth that way). Determinism is
                        // worth 32 MB per position at this scale, and a million labels would change the maths.
                        let mut searcher = Searcher::new();
                        searcher.config = Config::default();
                        let static_eval = high5::eval::evaluate(board, *us);
                        let r = searcher
                            .search(board, *us, Limits { time: Duration::from_secs(3600), max_depth: depth })
                            .unwrap();
                        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        if n % 50 == 0 {
                            eprintln!("  labelled {n}");
                        }
                        // A mate score is not a residual: the count model has no vocabulary for "mated in
                        // n", so those positions would measure it against a question it cannot answer.
                        if r.score.abs() >= 29_000 {
                            continue;
                        }
                        out.push((name.clone(), static_eval, r.score, r.score - static_eval, (board.stones[0].count_ones() + board.stones[1].count_ones()) as usize));
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let rows: Vec<(String, i32, i32, i32, usize)> = parts.into_iter().flatten().collect();
    let decided = positions.len() - rows.len();

    let n = rows.len() as f64;
    let (mx, my) = (
        rows.iter().map(|r| r.1 as f64).sum::<f64>() / n,
        rows.iter().map(|r| r.2 as f64).sum::<f64>() / n,
    );
    let (mut sxy, mut sxx, mut syy, mut mse) = (0.0, 0.0, 0.0, 0.0);
    for r in &rows {
        let (dx, dy) = (r.1 as f64 - mx, r.2 as f64 - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
        mse += (r.3 as f64) * (r.3 as f64);
    }
    let corr = if sxx > 0.0 && syy > 0.0 { sxy / (sxx.sqrt() * syy.sqrt()) } else { f64::NAN };
    println!(
        "corpus {n:.0} positions from {dir} (stride {stride}, depth {depth}): r = {corr:.3}, sd(residual) = {:.0}, mean = {:.0}, {decided} decided removed",
        (mse / n).sqrt(),
        rows.iter().map(|r| r.3 as f64).sum::<f64>() / n
    );
    let mut by_phase = [0; 4];
    let mut phase_res: [f64; 4] = [0.0; 4];
    for r in &rows {
        let p = (r.4 / 10).min(3);
        by_phase[p] += 1;
        phase_res[p] += r.3 as f64;
    }
    for (i, label) in ["<10 stones", "10-19", "20-29", "30+"] .iter().enumerate() {
        if by_phase[i] > 0 {
            println!("  {label:<11} n={:>4}  mean residual {:.0}", by_phase[i], phase_res[i] / by_phase[i] as f64);
        }
    }
    // What a fitted feature would have to explain. Most of the corpus agrees to within a few hundred points
    // and a small tail does not, so sizing the tail decides whether a feature is worth fitting at all: if the
    // spread is all tail, one term can absorb it; if it is broad, no single term can.
    // An empty corpus is a real outcome, not an error: at a deep enough labelling depth every sampled
    // position is decided, and the distribution below would index into nothing. Say so and stop.
    if rows.is_empty() {
        println!("no usable positions: all {decided} sampled positions are decided at this depth");
        return;
    }
    let mut abs: Vec<i32> = rows.iter().map(|r| r.3.abs()).collect();
    abs.sort_unstable();
    let q = |p: f64| abs[((abs.len() as f64 - 1.0) * p) as usize];
    let big = |t: i32| rows.iter().filter(|r| r.3.abs() > t).count();
    println!(
        "|residual| p50 {}  p90 {}  p99 {}   over 1000: {} ({:.0}%)  over 1500: {} ({:.0}%)  over 2000: {}",
        q(0.50), q(0.90), q(0.99),
        big(1000), 100.0 * big(1000) as f64 / n,
        big(1500), 100.0 * big(1500) as f64 / n,
        big(2000)
    );
    let tail: Vec<_> = rows.iter().filter(|r| r.3.abs() > 1500).collect();
    let bulk: Vec<_> = rows.iter().filter(|r| r.3.abs() <= 1500).collect();
    let mean_of = |v: &[&(String, i32, i32, i32, usize)], f: fn(&(String, i32, i32, i32, usize)) -> i64| {
        if v.is_empty() { f64::NAN } else { v.iter().map(|r| f(*r) as f64).sum::<f64>() / v.len() as f64 }
    };
    for (label, v) in [("tail >1500", &tail), ("bulk <=1500", &bulk)] {
        println!(
            "{label:<11} n={:>4}  mean stones {:.0}  mean static {:+.0}  share static<0 {:.0}%",
            v.len(),
            mean_of(v, |r| r.4 as i64),
            mean_of(v, |r| r.1 as i64),
            100.0 * v.iter().filter(|r| r.1 < 0).count() as f64 / v.len().max(1) as f64
        );
    }
    let mut sorted = rows.clone();
    sorted.sort_by_key(|r| -r.3.abs());
    println!("worst residuals:");
    for r in sorted.iter().take(5) {
        println!("  {:<28} static {:>7}  search {:>7}  residual {:>7}", r.0, r.1, r.2, r.3);
    }
}
