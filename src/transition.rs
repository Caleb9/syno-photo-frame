use std::time::Instant;

use sdl2::pixels::Color;

use crate::sdl;

#[derive(Debug)]
pub enum Transition {
    In,
    Out,
}

/// Possibly parametrize this and take command line argument to control length of the transition
const TRANSITION_DURATION_SECS: f64 = 2f64;
const TRANSITION_ALPHA_MIN: f64 = 0f64;
const TRANSITION_ALPHA_MAX: f64 = 255f64;

impl Transition {
    pub fn play(&self, sdl: &mut impl sdl::Sdl) -> Result<(), String> {
        let mut delta;
        let mut alpha = self.init_alpha();
        let mut last = Instant::now();
        while !self.is_finished(alpha) {
            delta = (Instant::now() - last).as_secs_f64();
            last = Instant::now();
            if super::is_exit_requested(sdl) {
                break;
            }
            alpha += self.step_alpha(delta);
            sdl.copy_texture_to_canvas()?;
            sdl.fill_canvas(Color::RGBA(0, 0, 0, f64::round(alpha) as u8))?;
            sdl.present_canvas();
        }
        Ok(())
    }

    fn init_alpha(&self) -> f64 {
        match self {
            Transition::In => TRANSITION_ALPHA_MAX,
            Transition::Out => TRANSITION_ALPHA_MIN,
        }
    }

    fn is_finished(&self, alpha: f64) -> bool {
        match self {
            Transition::In => alpha <= TRANSITION_ALPHA_MIN,
            Transition::Out => alpha >= TRANSITION_ALPHA_MAX,
        }
    }

    fn step_alpha(&self, delta: f64) -> f64 {
        const DIFF: f64 = TRANSITION_ALPHA_MAX / (TRANSITION_DURATION_SECS / 2f64);
        let diff = delta * DIFF;
        match self {
            Transition::In => -diff,
            Transition::Out => diff,
        }
    }
}
