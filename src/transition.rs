#[cfg(not(test))]
use std::time::Instant;

#[cfg(test)]
use mock_instant::Instant;

use anyhow::Result;

use crate::{
    cli::Transition,
    sdl::{Color, Sdl, TextureIndex},
};

const TRANSITION_ALPHA_MIN: f64 = 0_f64;
const TRANSITION_ALPHA_MAX: f64 = 255_f64;
// Possibly parametrize this and take command line argument to control length of the transition
const FADE_TO_BLACK_DURATION_SECS: f64 = 1_f64;
const CROSSFADE_DURATION_SECS: f64 = 1_f64;

impl Transition {
    pub fn play(&self, sdl: &mut impl Sdl) -> Result<()> {
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

    fn crossfade(&self, sdl: &mut impl Sdl) -> Result<()> {
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
    fn fade_to_black(&self, sdl: &mut impl Sdl, phase: FadeToBlackPhase) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mock_instant::MockClock;
    use mockall::Sequence;

    use crate::sdl::MockSdl;

    use super::*;

    #[test]
    fn fade_to_black_play_calls_canvas_methods_in_sequence() {
        let mut sdl = MockSdl::default();
        /* First iteration steps alpha by 0 */
        const EXPECTED_PHASE_ITERATIONS: usize = 16;

        sdl.expect_handle_quit_event()
            .times(2 * EXPECTED_PHASE_ITERATIONS)
            .returning(|| Ok(()));
        let mut sdl_seq = Sequence::default();
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        for texture_index in [&TextureIndex::Current, &TextureIndex::Next] {
            for _ in 0..EXPECTED_PHASE_ITERATIONS {
                sdl.expect_copy_texture_to_canvas()
                    .withf(move |index| index == texture_index)
                    .once()
                    .in_sequence(&mut sdl_seq)
                    .return_once(|_| Ok(()));
                sdl.expect_fill_canvas()
                    .once()
                    .in_sequence(&mut sdl_seq)
                    .return_once(|_| Ok(()));
                sdl.expect_present_canvas()
                    .once()
                    .in_sequence(&mut sdl_seq)
                    .returning(move || {
                        /* Simulate time passing between calls to Instant::now(), i.e. time it takes to process and
                         * display a frame. Here we pretend that approximately 30 FPS can be achieved. */
                        MockClock::advance(frame_duration)
                    });
            }
        }

        let result = Transition::FadeToBlack.play(&mut sdl);

        assert!(result.is_ok());
        sdl.checkpoint();
    }

    #[test]
    fn crossfade_play_calls_canvas_methods_in_sequence() {
        let mut sdl = MockSdl::default();
        /* First iteration steps alpha by 0 */
        const EXPECTED_ITERATIONS: usize = 31;
        sdl.expect_handle_quit_event()
            .times(EXPECTED_ITERATIONS)
            .returning(|| Ok(()));
        let mut sdl_seq = Sequence::default();
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        for _ in 0..EXPECTED_ITERATIONS {
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Current)
                .once()
                .in_sequence(&mut sdl_seq)
                .return_once(|_| Ok(()));
            sdl.expect_set_texture_alpha()
                .once()
                .in_sequence(&mut sdl_seq)
                .return_const(());
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Next)
                .once()
                .in_sequence(&mut sdl_seq)
                .return_once(|_| Ok(()));
            sdl.expect_present_canvas()
                .once()
                .in_sequence(&mut sdl_seq)
                .returning(move || {
                    /* Simulate time passing between calls to Instant::now(), i.e. time it takes to process and
                     * display a frame. Here we pretend that approximately 30 FPS can be achieved. */
                    MockClock::advance(frame_duration)
                });
        }

        let result = Transition::Crossfade.play(&mut sdl);

        assert!(result.is_ok());
        sdl.checkpoint();
    }

    #[test]
    fn fade_to_black_play_takes_one_second_and_is_fps_independent() {
        test_case(30_f64);
        test_case(60_f64);

        fn test_case(fps: f64) {
            let mut sdl = MockSdl::default();
            sdl.expect_handle_quit_event().returning(|| Ok(()));
            let frame_duration = Duration::from_secs_f64(1_f64 / fps);
            sdl.expect_copy_texture_to_canvas().returning(|_| Ok(()));
            sdl.expect_fill_canvas().returning(|_| Ok(()));
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            Transition::FadeToBlack.play(&mut sdl).unwrap();

            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_secs(), 1);
        }
    }

    #[test]
    fn crossfade_play_takes_one_second_and_is_fps_independent() {
        test_case(30_f64);
        test_case(60_f64);

        fn test_case(fps: f64) {
            let mut sdl = MockSdl::default();
            sdl.expect_handle_quit_event().returning(|| Ok(()));
            let frame_duration = Duration::from_secs_f64(1_f64 / fps);
            sdl.expect_copy_texture_to_canvas().returning(|_| Ok(()));
            sdl.expect_set_texture_alpha().return_const(());
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            Transition::Crossfade.play(&mut sdl).unwrap();

            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_secs(), 1);
        }
    }

    #[test]
    fn fade_to_black_play_mutates_alpha() {
        let mut sdl = MockSdl::default();
        sdl.expect_handle_quit_event().returning(|| Ok(()));
        sdl.expect_copy_texture_to_canvas().returning(|_| Ok(()));
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        let mut sdl_seq = Sequence::default();
        let alpha_prefix = [0, 17, 34];
        for alpha in alpha_prefix {
            /* Check alpha value for first 3 calls to fill_canvas. */
            sdl.expect_fill_canvas()
                .withf(move |color| *color == Color::RGBA(0, 0, 0, alpha))
                .once()
                .in_sequence(&mut sdl_seq)
                .return_once(|_| Ok(()));
        }
        /* Set up calls between first and last 3 iterations */
        const EXPECTED_ITERATIONS: usize = 32;
        let alpha_postfix = [34, 17, 0];
        sdl.expect_fill_canvas()
            .times(EXPECTED_ITERATIONS - alpha_prefix.len() - alpha_postfix.len())
            .returning(|_| Ok(()));
        for alpha in alpha_postfix {
            /* Check alpha value for last 3 calls to fill_canvas. */
            sdl.expect_fill_canvas()
                .withf(move |color| *color == Color::RGBA(0, 0, 0, alpha))
                .once()
                .in_sequence(&mut sdl_seq)
                .return_once(|_| Ok(()));
        }
        sdl.expect_present_canvas()
            .returning(move || MockClock::advance(frame_duration));

        Transition::FadeToBlack.play(&mut sdl).unwrap();

        sdl.checkpoint();
    }

    #[test]
    fn crossfade_play_mutates_alpha() {
        let mut sdl = MockSdl::default();
        sdl.expect_handle_quit_event().returning(|| Ok(()));
        sdl.expect_copy_texture_to_canvas().returning(|_| Ok(()));
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        let mut sdl_seq = Sequence::default();
        let alpha_prefix: [u8; 3] = [0, 8, 17];
        for alpha in alpha_prefix {
            /* Check alpha value for first 3 calls to fill_canvas. */
            sdl.expect_set_texture_alpha()
                .withf(move |a, i| a == &alpha && i == &TextureIndex::Next)
                .once()
                .in_sequence(&mut sdl_seq)
                .return_const(());
        }
        /* Set up calls between first and last 3 iterations */
        const EXPECTED_ITERATIONS: usize = 31;
        let alpha_postfix: [u8; 3] = [238, 246, 255];
        sdl.expect_set_texture_alpha()
            .times(EXPECTED_ITERATIONS - alpha_prefix.len() - alpha_postfix.len())
            .in_sequence(&mut sdl_seq)
            .return_const(());
        for alpha in alpha_postfix {
            /* Check alpha value for last 3 calls to fill_canvas. */
            sdl.expect_set_texture_alpha()
                .withf(move |a, i| a == &alpha && i == &TextureIndex::Next)
                .once()
                .in_sequence(&mut sdl_seq)
                .return_const(());
        }
        sdl.expect_present_canvas()
            .returning(move || MockClock::advance(frame_duration));

        Transition::Crossfade.play(&mut sdl).unwrap();

        sdl.checkpoint();
    }

    fn reset_clock() {
        MockClock::set_time(Duration::ZERO);
    }
}
