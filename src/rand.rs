use std::ops::Range;

use rand::{prelude::SliceRandom, rng, Rng};

pub trait Random {
    fn random_range(&self, range: Range<u32>) -> u32 {
        rng().random_range(range)
    }

    fn shuffle<T>(&self, slice: &mut [T]) {
        slice.shuffle(&mut rng())
    }
}

pub struct RandomImpl;

impl Random for RandomImpl {}
