//! Dumps per-position rows from the recorded games for the Rapfi-labelling pass: `name#ply us=<board
//! player> static=<our eval> stones=<p:x,y ...>` plus the fit features as `k=v` tokens.
//!
//! Usage: cargo run --release --example eval_dump [-- <dir> <stride> <min_ply>]
//! Board player 0 is Piskvork's colour 1, so Rapfi infers the same side to move we did. A stride must be
//! odd to sample both colours, or every sampled ply count is even and player 0 is to move in all of them.

use slap5::Board;
use slap5::bitboard::{Bits, DIRS, STRIDE};
use slap5::eval;
use slap5::threats::{FOUR, OPEN_FOUR, OPEN_THREE, SHAPES};

/// The eight neighbour offsets in cells; steps off either edge land on the always-empty guard column.
const OFFSETS: [i32; 8] = [-1, 1, -16, 16, -17, 17, -15, 15];

/// The counts the evaluation is a linear function of, plus a few candidates, as `k=v` tokens, so a weight
/// set can be priced without rebuilding the engine. Per side (`u` the mover, `t` the other): `g{c}` class
/// `c` as groups per direction, `v{c}` as cells, `cen` centrality, `adj` same-side neighbour pairs; `tp` is
/// +1 when board player 0 moves. Names are read by `engine/scripts/fit_eval.py`.
fn features(board: &Board, us: usize) -> String {
    let shapes = board.shapes();
    let mut out = String::new();
    for (side, tag) in [(us, 'u'), (1 - us, 't')] {
        for class in 0..SHAPES {
            let total: i32 = (0..4).map(|i| eval::groups(shapes.ranked(side)[i][class], DIRS[i])).sum();
            out.push_str(&format!("g{tag}{class}={total} "));
        }
        for class in 0..SHAPES {
            out.push_str(&format!("v{tag}{class}={} ", shapes.cells(side, class).count_ones()));
        }
        // Cells of one class that more than one direction contributes: the double-threat motif. The union
        // has to be taken over the ranked masks themselves - `cells` unions the raw ones, before the
        // precedence that moves a cell into its strongest class, so differencing against it can go negative.
        for class in 0..SHAPES {
            let mut union = shapes.ranked(side)[0][class];
            for i in 1..4 {
                union = union.or(shapes.ranked(side)[i][class]);
            }
            let per_dir: u32 = (0..4).map(|i| shapes.ranked(side)[i][class].count_ones()).sum();
            out.push_str(&format!("o{tag}{class}={} ", per_dir as i32 - union.count_ones() as i32));
        }
        // The per-class unions summed, and the union over everything. Their difference counts cells that
        // create shapes of *different* classes in different directions - a four one way and an open three
        // another - which the per-class overlap could never see.
        let mut per_class = 0;
        let mut all = shapes.ranked(side)[0][0];
        for class in 0..SHAPES {
            let mut union = shapes.ranked(side)[0][class];
            for i in 1..4 {
                union = union.or(shapes.ranked(side)[i][class]);
            }
            per_class += union.count_ones();
            all = all.or(union);
        }
        out.push_str(&format!("xu{tag}={per_class} xa{tag}={} ", all.count_ones()));
        // Directions holding a four or better, and holding an open three.
        let df = (0..4).filter(|&i| !shapes.ranked(side)[i][FOUR].is_zero() || !shapes.ranked(side)[i][OPEN_FOUR].is_zero()).count();
        let dt = (0..4).filter(|&i| !shapes.ranked(side)[i][OPEN_THREE].is_zero()).count();
        out.push_str(&format!("xf{tag}={df} xt{tag}={dt} "));
        let (mut central, mut pairs) = (0, 0);
        for cell in board.stones[side].bits() {
            let (r, c) = (cell / STRIDE, cell % STRIDE);
            let (dx, dy) = ((c as i32 - 7).unsigned_abs(), (r as i32 - 7).unsigned_abs());
            central += 7 - dx.max(dy) as i32;
            for off in OFFSETS {
                let j = cell as i32 + off;
                if j >= 0 && board.stones[side].get(j as usize) {
                    pairs += 1;
                }
            }
        }
        // The same neighbour-pair count by shifting the whole mask, which is the form the engine could
        // afford; the two must agree, and the corpus is where that gets checked.
        let mut shifted = 0;
        for (shrink, off) in [(true, 1u32), (false, 1), (true, 16), (false, 16), (true, 15), (false, 15), (true, 17), (false, 17)] {
            let m = if shrink { board.stones[side].shr(off) } else { board.stones[side].shl(off) };
            shifted += board.stones[side].and(m).count_ones();
        }
        out.push_str(&format!("cen{tag}={central} adj{tag}={} sh{tag}={} ", pairs / 2, shifted / 2));
    }
    out.push_str(&format!("tp={}", if us == 0 { 1 } else { -1 }));
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = args.first().cloned().unwrap_or_else(|| "logs".to_string());
    let stride: usize = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let min_ply: usize = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(6);

    // A run is a directory of `.psq` files, and the input is either one run or a directory of runs. One run
    // keeps bare stems, so a reader keyed by `(name, ply)` sees the names its results.csv uses (this is what
    // `fit_outcome.py` dumps, one run at a time). A directory of runs prefixes the run's own directory name,
    // because every run reuses names like `p00_slap5_first` and a reader would otherwise overwrite one run's
    // rows with another's.
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("run directory").filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            let run = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            for f in std::fs::read_dir(&p).into_iter().flatten().filter_map(|e| e.ok()) {
                let f = f.path();
                if f.extension().map(|x| x == "psq").unwrap_or(false) {
                    files.push((run.clone(), f));
                }
            }
        } else if p.extension().map(|x| x == "psq").unwrap_or(false) {
            files.push((String::new(), p));
        }
    }
    files.sort();

    for (run, path) in files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let name = if run.is_empty() { stem } else { format!("{run}/{stem}") };
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut board = Board::new();
        let mut stones: Vec<(usize, usize, usize)> = Vec::new();
        let mut ply = 0usize;
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
            let p = ply % 2;
            board.make(p, y, x);
            stones.push((p, x, y));
            ply += 1;
            // Quiet positions, past the opening: a position where either side can complete five is decided.
            if ply < min_ply
                || ply % stride != 0
                || !board.five_cells(0).is_zero()
                || !board.five_cells(1).is_zero()
            {
                continue;
            }
            let us = 1 - p; // the side to move after this stone
            let eval = slap5::eval::evaluate(&board, us);
            let s: Vec<String> = stones.iter().map(|&(pp, x, y)| format!("{pp}:{x},{y}")).collect();
            // Features follow the stones as `k=v` tokens; the label pass reads stones up to the first one.
            println!("{}#{} us={} static={} stones={} {}", name, ply, us, eval, s.join(" "), features(&board, us));
        }
    }
}
