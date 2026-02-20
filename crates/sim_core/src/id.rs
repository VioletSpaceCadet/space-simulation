use rand::Rng;
use uuid::Uuid;

/// Generate a deterministic v4-format UUID from a seeded RNG.
pub fn generate_uuid(rng: &mut impl Rng) -> Uuid {
    let bytes: [u8; 16] = rng.gen();
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn deterministic_uuid_from_same_seed() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);
        let id1 = generate_uuid(&mut rng1);
        let id2 = generate_uuid(&mut rng2);
        assert_eq!(id1, id2);
        assert_eq!(id1.get_version(), Some(uuid::Version::Random));
    }

    #[test]
    fn different_seeds_produce_different_uuids() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(99);
        let id1 = generate_uuid(&mut rng1);
        let id2 = generate_uuid(&mut rng2);
        assert_ne!(id1, id2);
    }
}
