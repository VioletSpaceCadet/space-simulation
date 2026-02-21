use super::*;

#[test]
fn tick_throughput_exceeds_100k_per_second() {
    let content = test_content();
    let mut state = test_state(&content);
    let mut rng = make_rng();

    let tick_count = 50_000u64;

    let start = std::time::Instant::now();
    for _ in 0..tick_count {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }
    let elapsed = start.elapsed();

    let ticks_per_sec = tick_count as f64 / elapsed.as_secs_f64();
    assert!(
        ticks_per_sec >= 100_000.0,
        "expected >= 100k ticks/sec, got {ticks_per_sec:.0} ticks/sec ({tick_count} ticks in {elapsed:.2?})"
    );
}
