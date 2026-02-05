//! E7.2: Determinism of test order shuffle by seed.
//!
//! Same order_seed must produce the same test order (StdRng + SliceRandom),
//! so that replay with the same seed is deterministic.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

fn shuffle_order(ids: &[&str], seed: u64) -> Vec<String> {
    let mut order: Vec<String> = ids.iter().map(|s| (*s).to_string()).collect();
    let mut rng = StdRng::seed_from_u64(seed);
    order.shuffle(&mut rng);
    order
}

#[test]
fn same_seed_same_order() {
    let ids = ["a", "b", "c", "d", "e"];
    let seed = 42u64;
    let order1 = shuffle_order(&ids, seed);
    let order2 = shuffle_order(&ids, seed);
    assert_eq!(
        order1, order2,
        "same seed must yield identical shuffle order"
    );
}

#[test]
fn different_seed_may_differ() {
    let ids = ["a", "b", "c", "d", "e"];
    let order1 = shuffle_order(&ids, 1);
    let order2 = shuffle_order(&ids, 2);
    // With high probability they differ (not guaranteed for all RNG outputs)
    let same = order1 == order2;
    if !same {
        return; // desired: different seeds => different order
    }
    // If by chance equal, try another pair of seeds
    let order3 = shuffle_order(&ids, 3);
    assert_ne!(
        order1, order3,
        "different seeds should typically yield different order"
    );
}
