//! Reproducible audit probes, without changing production search.
//! cargo run --release --example audit
use high5::bitboard::{Bits, STRIDE};
use high5::search::{Config, Limits, Searcher};
use high5::threats;
use high5::{Board, eval};
use std::time::Duration;

fn board(stones: &[(usize, usize, usize)]) -> Board {
    let mut b = Board::new();
    for &(p, x, y) in stones {
        b.make(p, y, x);
    }
    b
}

fn main() {
    for (name, stones) in [
        ("centre singleton", vec![(0, 7, 7)]),
        ("corner singleton", vec![(0, 0, 0)]),
        ("blocked pair", vec![(0, 6, 7), (0, 7, 7), (1, 5, 7)]),
        ("open pair", vec![(0, 6, 7), (0, 7, 7)]),
        (
            "one blocked three",
            vec![(0, 4, 7), (0, 5, 7), (0, 6, 7), (1, 3, 7)],
        ),
    ] {
        println!("eval {name}: {}", eval::evaluate(&board(&stones), 0));
    }
    // Same target is one four-cell bit whether it makes one or two independent fours.
    let one = board(&[(0, 4, 7), (0, 5, 7), (0, 6, 7), (1, 3, 7)]);
    let two = board(&[
        (0, 4, 7),
        (0, 5, 7),
        (0, 6, 7),
        (1, 3, 7),
        (0, 7, 4),
        (0, 7, 5),
        (0, 7, 6),
        (1, 7, 3),
    ]);
    // Largest evaluation over random 30-stone positions: the clamp in `eval` must stay clear of it.
    let mut seed = 0x1234_5678_9ABC_DEF0u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 3) as usize
    };
    let mut peak = 0;
    for _ in 0..200 {
        let mut stones = Vec::new();
        while stones.len() < 30 {
            let (x, y) = (next() % 15, next() % 15);
            if !stones.iter().any(|&(_, sx, sy)| (x, y) == (sx, sy)) {
                stones.push((stones.len() % 2, x, y));
            }
        }
        peak = peak.max(eval::evaluate(&board(&stones), 0).abs());
    }
    println!("peak |eval| over 200 random 30-stone positions: {peak} (clamp {})", eval::MAX);

    let target = 7 * STRIDE + 7;
    for (name, b) in [("single four", one), ("double four", two)] {
        let shapes = b.shapes();
        let dirs = |class: usize| (0..4).filter(|&k| shapes.ranked(0)[k][class].get(target)).count();
        println!(
            "target {name}: cells four/open4/open3 = {}/{}/{}; directions making a four {}",
            b.four_cells(0).get(target),
            b.open_four_cells(0).get(target),
            b.open_three_cells(0).get(target),
            dirs(threats::FOUR)
        );
    }

    let cfg = Config {
        use_tt: false,
        free_three_replies: false,
        free_four_replies: false,
        selective: false,
        ..Config::default()
    };
    let mut seed = 0x5555_AAAA_1234_9876u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 3) as usize
    };
    let (mut total, mut different, mut move_different, mut printed, mut early) = (0, 0, 0, 0, 0);
    let (mut exact_nodes, mut selective_nodes) = (0, 0);
    for index in 0..60 {
        let mut stones = Vec::new();
        while stones.len() < 8 {
            let (x, y) = (4 + next() % 6, 4 + next() % 6);
            if !stones.iter().any(|&(_, sx, sy)| (x, y) == (sx, sy)) {
                stones.push((stones.len() % 2, x, y));
            }
        }
        let b = board(&stones);
        let run = |selective| {
            let mut s = Searcher::with_table_bits(16);
            s.config = Config { selective, ..cfg };
            s.search(
                &b,
                0,
                Limits {
                    time: Duration::from_secs(30),
                    max_depth: 3,
                },
            )
            .unwrap()
        };
        let exact = run(false);
        let selected = run(true);
        if exact.depth != 3 || selected.depth != 3 {
            early += 1;
            println!(
                "early mate {index}: exact depth={} score={} selective depth={} score={}",
                exact.depth, exact.score, selected.depth, selected.score
            );
            continue;
        }
        total += 1;
        exact_nodes += exact.stats.nodes;
        selective_nodes += selected.stats.nodes;
        move_different += ((exact.x, exact.y) != (selected.x, selected.y)) as usize;
        if exact.score != selected.score {
            different += 1;
            if printed < 5 {
                printed += 1;
                println!(
                    "mismatch {index}: exact=({},{}):{} selective=({},{}):{} stones={stones:?}",
                    exact.x, exact.y, exact.score, selected.x, selected.y, selected.score
                );
            }
        }
    }
    println!(
        "fixed depth 3, isolated selectivity: {different}/{total} scores differ; {move_different}/{total} moves differ; exact_nodes={exact_nodes} selective_nodes={selective_nodes}"
    );
    println!("excluded {early} positions with early mate exits");
}
