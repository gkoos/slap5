//! Replays recorded games with one table kept across the moves, as in a real game, and reports for each
//! table size how full the table runs: lookups that found their position, and stores that evicted a
//! position written during the same search. If the small table evicts little, a larger one cannot help.
//! Usage: cargo run --release --example tt_replay -- <openings file> <ms per move> <bits,bits,...> <game.psq>...
//! Records are match.ps1/judge.ps1 exports: `pNN_slap5_first.psq` has slap5 as black, `pNN_opponent_first.psq`
//! as white, and NN is the pair, whose line of the openings file the run's `run-info.txt` lists.

use slap5::Board;
use slap5::bitboard::cell;
use slap5::search::{Limits, Searcher};
use std::path::Path;
use std::time::Duration;

/// The openings-file line of each pair of the run that wrote `record`, from the `pairs: N; openings: a,b,...`
/// line of the `run-info.txt` beside it.
fn pair_openings(record: &Path) -> Vec<usize> {
    let info = record.parent().expect("record directory").join("run-info.txt");
    let text = std::fs::read_to_string(&info).unwrap_or_else(|_| panic!("{} is missing", info.display()));
    let list = text
        .lines()
        .find_map(|l| l.strip_prefix("pairs: ").and_then(|r| r.split_once("; openings: ")).map(|(_, o)| o))
        .unwrap_or_else(|| panic!("{} has no `pairs: N; openings: ...` line", info.display()));
    list.trim().split(',').map(|i| i.parse().expect("opening index")).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 4, "usage: tt_replay <openings file> <ms> <bits,bits> <game.psq>...");
    let stones: Vec<usize> = std::fs::read_to_string(&args[0])
        .expect("openings file")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split(',').filter(|t| !t.trim().is_empty()).count() / 2)
        .collect();
    let ms: u64 = args[1].parse().expect("ms");
    let sizes: Vec<u32> = args[2].split(',').map(|b| b.parse().expect("bits")).collect();
    for path in &args[3..] {
        let name = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        let pair: usize = name[1..3].parse().expect("pNN_ record name");
        let opening = stones[pair_openings(Path::new(path))[pair]];
        let us = if name.contains("_slap5_first") { 0 } else { 1 };
        let moves: Vec<(usize, usize)> = std::fs::read_to_string(path)
            .expect("record")
            .lines()
            .filter_map(|l| {
                let p: Vec<&str> = l.trim().split(',').collect();
                if p.len() != 3 {
                    return None;
                }
                Some((p[0].parse::<usize>().ok()? - 1, p[1].parse::<usize>().ok()? - 1))
            })
            .collect();
        for &bits in &sizes {
            let mut searcher = Searcher::with_table_bits(bits);
            let mut board = Board::new();
            let mut played: Vec<u8> = Vec::new();
            let (mut searched, mut depth_sum, mut nodes, mut probes, mut hits, mut stores, mut evictions) = (0, 0, 0, 0, 0, 0, 0);
            for (i, &(x, y)) in moves.iter().enumerate() {
                let side = i % 2;
                if side == us && i >= opening {
                    let limits = Limits { time: Duration::from_millis(ms), max_depth: 100 };
                    let r = searcher.search_with(&board, us, limits, &played).unwrap();
                    searched += 1;
                    depth_sum += r.depth as u64;
                    nodes += r.stats.nodes;
                    probes += r.stats.tt_probes;
                    hits += r.stats.tt_hits;
                    stores += r.stats.tt_stores;
                    evictions += r.stats.tt_evictions;
                }
                board.make(side, y, x);
                played.push(cell(x, y) as u8);
            }
            let pct = |a: u64, b: u64| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
            println!(
                "{name:<26} 2^{bits}: {searched:>2} moves, mean depth {:.2}, nodes {nodes:>11}, hits {:.1}% of lookups, evictions {:.1}% of stores",
                depth_sum as f64 / searched.max(1) as f64,
                pct(hits, probes),
                pct(evictions, stores)
            );
        }
    }
}
