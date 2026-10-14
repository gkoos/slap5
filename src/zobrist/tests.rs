use super::*;

#[test]
fn keys_are_distinct_and_nonzero() {
    let mut all: Vec<u64> = (0..2).flat_map(|p| (0..256).map(move |i| key(p, i))).collect();
    assert!(all.iter().all(|&k| k != 0));
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 512);
}
