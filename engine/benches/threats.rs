use criterion::{Criterion, black_box, criterion_group, criterion_main};
use slap5::Board;
use slap5::bitboard::Bits;

const POSITIONS: usize = 256;
const STONES: usize = 30;

fn position(seed: u64) -> Board {
    let mut s = seed | 1;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let mut b = Board::new();
    let mut seen = [[false; 15]; 15];
    let mut n = 0;
    while n < STONES {
        let r = 3 + (next() % 9) as usize;
        let c = 3 + (next() % 9) as usize;
        if !seen[r][c] {
            seen[r][c] = true;
            b.make(n % 2, r, c);
            n += 1;
        }
    }
    b
}

fn masks(c: &mut Criterion) {
    let all: Vec<Board> = (1..=POSITIONS as u64).map(position).collect();
    let mut group = c.benchmark_group("per_position_both_players");

    macro_rules! bench {
        ($name:expr, $method:ident) => {
            group.bench_function($name, |bn| {
                bn.iter(|| {
                    let mut n = 0u32;
                    for b in &all {
                        n += b.$method(0).count_ones() + b.$method(1).count_ones();
                    }
                    black_box(n)
                })
            });
        };
    }

    bench!("five_cells", five_cells);
    bench!("four_cells", four_cells);
    bench!("open_four_cells", open_four_cells);
    bench!("open_three_cells", open_three_cells);
    bench!("open_four_block_cells", open_four_block_cells);

    group.bench_function("all_five_masks", |bn| {
        bn.iter(|| {
            let mut n = 0u32;
            for b in &all {
                for p in 0..2 {
                    n += b.five_cells(p).count_ones()
                        + b.four_cells(p).count_ones()
                        + b.open_four_cells(p).count_ones()
                        + b.open_three_cells(p).count_ones()
                        + b.open_four_block_cells(p).count_ones();
                }
            }
            black_box(n)
        })
    });

    group.bench_function("make_unmake_one_stone", |bn| {
        let mut b = all[0];
        bn.iter(|| {
            b.make(0, 1, 1);
            b.unmake(0, 1, 1);
            black_box(&b);
        })
    });
    group.finish();
}

criterion_group!(benches, masks);
criterion_main!(benches);
