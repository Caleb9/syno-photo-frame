use std::fmt::{Display, Formatter};

#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::Instant;

use crate::{
    cli::Transition,
    sdl::{Color, Sdl, TextureIndex},
    QuitEvent,
};

const TRANSITION_ALPHA_MIN: f64 = 0_f64;
const TRANSITION_ALPHA_MAX: f64 = 255_f64;
// Possibly parametrize this and take command line argument to control length of the transition
const FADE_TO_BLACK_DURATION_SECS: f64 = 1_f64;
const CROSSFADE_DURATION_SECS: f64 = 1_f64;

#[derive(Debug)]
pub enum TransitionError {
    Sdl(String),
    Quit(QuitEvent),
}

impl Transition {
    pub fn play(&self, sdl: &mut impl Sdl) -> Result<(), TransitionError> {
        match self {
            Transition::Crossfade => {
                self.crossfade(sdl)?;
            }
            Transition::FadeToBlack => {
                self.fade_to_black(sdl, FadeToBlackPhase::Out)?;
                self.fade_to_black(sdl, FadeToBlackPhase::In)?;
            }
            Transition::None => {
                sdl.copy_texture_to_canvas(TextureIndex::Next)?;
                sdl.present_canvas();
            }
        }
        Ok(())
    }

    fn crossfade(&self, sdl: &mut impl Sdl) -> Result<(), TransitionError> {
        let mut delta;
        let mut alpha = TRANSITION_ALPHA_MIN;
        let mut last = Instant::now();
        const DIFF: f64 = TRANSITION_ALPHA_MAX / CROSSFADE_DURATION_SECS;
        while alpha.round() < TRANSITION_ALPHA_MAX {
            sdl.handle_quit_event()?;
            delta = (Instant::now() - last).as_secs_f64();
            last = Instant::now();
            sdl.copy_texture_to_canvas(TextureIndex::Current)?;
            alpha += delta * DIFF;
            sdl.set_texture_alpha(alpha.round() as u8, TextureIndex::Next);
            sdl.copy_texture_to_canvas(TextureIndex::Next)?;
            sdl.present_canvas();
        }
        Ok(())
    }

    /// Returns false if exit event occurred
    fn fade_to_black(
        &self,
        sdl: &mut impl Sdl,
        phase: FadeToBlackPhase,
    ) -> Result<(), TransitionError> {
        let mut delta;
        let mut alpha = phase.init_alpha();
        let mut last = Instant::now();
        while !phase.is_finished(alpha) {
            sdl.handle_quit_event()?;
            delta = (Instant::now() - last).as_secs_f64();
            last = Instant::now();
            alpha += phase.step_alpha(delta);
            sdl.copy_texture_to_canvas(phase.texture_index())?;
            sdl.fill_canvas(Color::RGBA(0, 0, 0, alpha.round() as u8))?;
            sdl.present_canvas();
        }
        Ok(())
    }
}

enum FadeToBlackPhase {
    Out,
    In,
}

impl FadeToBlackPhase {
    const fn init_alpha(&self) -> f64 {
        match self {
            FadeToBlackPhase::Out => TRANSITION_ALPHA_MIN,
            FadeToBlackPhase::In => TRANSITION_ALPHA_MAX,
        }
    }

    fn is_finished(&self, alpha: f64) -> bool {
        match self {
            FadeToBlackPhase::Out => alpha.round() >= TRANSITION_ALPHA_MAX,
            FadeToBlackPhase::In => alpha.round() <= TRANSITION_ALPHA_MIN,
        }
    }

    fn step_alpha(&self, delta: f64) -> f64 {
        const DIFF: f64 = TRANSITION_ALPHA_MAX / (FADE_TO_BLACK_DURATION_SECS / 2f64);
        let diff = delta * DIFF;
        match self {
            FadeToBlackPhase::Out => diff,
            FadeToBlackPhase::In => -diff,
        }
    }

    const fn texture_index(&self) -> TextureIndex {
        match self {
            FadeToBlackPhase::Out => TextureIndex::Current,
            FadeToBlackPhase::In => TextureIndex::Next,
        }
    }
}

impl Display for TransitionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::Sdl(error) => write!(f, "{error}"),
            TransitionError::Quit(quit) => write!(f, "{quit}"),
        }
    }
}

impl From<String> for TransitionError {
    fn from(value: String) -> Self {
        TransitionError::Sdl(value)
    }
}

impl From<QuitEvent> for TransitionError {
    fn from(value: QuitEvent) -> Self {
        TransitionError::Quit(value)
    }
}
