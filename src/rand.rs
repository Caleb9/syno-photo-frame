use std::ops::Range;

use rand::{prelude::SliceRandom, thread_rng, Rng};

pub trait Random {
    fn gen_range(&self, range: Range<u32>) -> u32 {
        thread_rng().gen_range(range)
    }

    fn shuffle<T>(&self, slice: &mut [T]) {
        slice.shuffle(&mut thread_rng())
    }
}

pub struct RandomImpl;

impl Random for RandomImpl {}
