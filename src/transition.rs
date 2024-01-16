#[cfg(test)]
use mock_instant::Instant;

#[cfg(not(test))]
use std::time::Instant;

use crate::{
    cli::Transition,
    sdl::{Color, Sdl, TextureIndex},
};

const TRANSITION_ALPHA_MIN: f64 = 0_f64;
const TRANSITION_ALPHA_MAX: f64 = 255_f64;
// Possibly parametrize this and take command line argument to control length of the transition
const FADE_TO_BLACK_DURATION_SECS: f64 = 2_f64;
const CROSSFADE_DURATION_SECS: f64 = 1_f64;

impl Transition {
    pub(crate) fn play(
        &self,
        sdl: &mut impl Sdl,
        show_update_notification: bool,
    ) -> Result<(), String> {
        match self {
            Transition::Crossfade => self.crossfade(sdl, show_update_notification),
            Transition::FadeToBlack => {
                if !self.fade_to_black(sdl, FadeToBlackPhase::Out, show_update_notification)?
                    || !self.fade_to_black(sdl, FadeToBlackPhase::In, show_update_notification)?
                {
                    return Ok(());
                }
                Ok(())
            }
            Transition::None => {
                sdl.copy_texture_to_canvas(TextureIndex::Next)?;
                if show_update_notification {
                    sdl.copy_update_notification_to_canvas()?;
                }
                sdl.present_canvas();
                Ok(())
            }
        }
    }

    fn crossfade(&self, sdl: &mut impl Sdl, show_update_notification: bool) -> Result<(), String> {
        let mut delta;
        let mut alpha = TRANSITION_ALPHA_MIN;
        let mut last = Instant::now();
        const DIFF: f64 = TRANSITION_ALPHA_MAX / CROSSFADE_DURATION_SECS;
        while alpha.round() < TRANSITION_ALPHA_MAX {
            delta = (Instant::now() - last).as_secs_f64();
            last = Instant::now();
            if super::is_exit_requested(sdl) {
                break;
            }
            sdl.copy_texture_to_canvas(TextureIndex::Current)?;
            alpha += delta * DIFF;
            sdl.set_texture_alpha(alpha.round() as u8, TextureIndex::Next);
            sdl.copy_texture_to_canvas(TextureIndex::Next)?;
            if show_update_notification {
                sdl.copy_update_notification_to_canvas()?;
            }
            sdl.present_canvas();
        }
        Ok(())
    }

    /// Returns false if exit event occurred
    fn fade_to_black(
        &self,
        sdl: &mut impl Sdl,
        phase: FadeToBlackPhase,
        show_update_notification: bool,
    ) -> Result<bool, String> {
        let mut delta;
        let mut alpha = phase.init_alpha();
        let mut last = Instant::now();
        while !phase.is_finished(alpha) {
            delta = (Instant::now() - last).as_secs_f64();
            last = Instant::now();
            if super::is_exit_requested(sdl) {
                return Ok(false);
            }
            alpha += phase.step_alpha(delta);
            sdl.copy_texture_to_canvas(phase.texture_index())?;
            sdl.fill_canvas(Color::RGBA(0, 0, 0, alpha.round() as u8))?;
            if show_update_notification {
                sdl.copy_update_notification_to_canvas()?;
            }
            sdl.present_canvas();
        }
        Ok(true)
    }
}

enum FadeToBlackPhase {
    Out,
    In,
}

impl FadeToBlackPhase {
    fn init_alpha(&self) -> f64 {
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

    fn texture_index(&self) -> TextureIndex {
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

    use crate::sdl::{Event, MockSdl};

    use super::*;

    #[test]
    fn fade_to_black_play_calls_canvas_methods_in_sequence() {
        let mut sdl = MockSdl::default();
        /* First iteration steps alpha by 0 */
        const EXPECTED_FADE_OUT_ITERATIONS: usize = 31;
        const EXPECTED_FADE_IN_ITERATIONS: usize = 31;
        sdl.expect_events()
            .times(EXPECTED_FADE_OUT_ITERATIONS + EXPECTED_FADE_IN_ITERATIONS)
            .returning(|| Box::new([].into_iter()));
        let mut canvas_seq = Sequence::default();
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        for _ in 0..EXPECTED_FADE_OUT_ITERATIONS {
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Current)
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_fill_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_copy_update_notification_to_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_present_canvas()
                    .once()
                    .in_sequence(&mut canvas_seq)
                    .returning(move || {
                        /* Simulate time passing between calls to Instant::now(), i.e. time it takes to process and
                         * display a frame. Here we pretend that approximately 30 FPS can be achieved. */
                        MockClock::advance(frame_duration)
                    });
        }
        for _ in 0..EXPECTED_FADE_IN_ITERATIONS {
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Next)
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_fill_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_copy_update_notification_to_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_present_canvas()
                    .once()
                    .in_sequence(&mut canvas_seq)
                    .returning(move || {
                        /* Simulate time passing between calls to Instant::now(), i.e. time it takes to process and
                         * display a frame. Here we pretend that approximately 30 FPS can be achieved. */
                        MockClock::advance(frame_duration)
                    });
        }

        let result = Transition::FadeToBlack.play(&mut sdl, true);

        assert!(result.is_ok());
        sdl.checkpoint();
    }

    #[test]
    fn crossfade_play_calls_canvas_methods_in_sequence() {
        let mut sdl = MockSdl::default();
        /* First iteration steps alpha by 0 */
        const EXPECTED_ITERATIONS: usize = 31;
        sdl.expect_events()
            .times(EXPECTED_ITERATIONS)
            .returning(|| Box::new([].into_iter()));
        let mut canvas_seq = Sequence::default();
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        for _ in 0..EXPECTED_ITERATIONS {
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Current)
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_set_texture_alpha()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(());
            sdl.expect_copy_texture_to_canvas()
                .withf(|index| index == &TextureIndex::Next)
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_copy_update_notification_to_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .return_const(Ok(()));
            sdl.expect_present_canvas()
                .once()
                .in_sequence(&mut canvas_seq)
                .returning(move || {
                    /* Simulate time passing between calls to Instant::now(), i.e. time it takes to process and
                     * display a frame. Here we pretend that approximately 30 FPS can be achieved. */
                    MockClock::advance(frame_duration)
                });
        }

        let result = Transition::Crossfade.play(&mut sdl, true);

        assert!(result.is_ok());
        sdl.checkpoint();
    }

    #[test]
    fn fade_to_black_play_takes_two_seconds_and_is_fps_independent() {
        test_case(30_f64);
        test_case(60_f64);

        fn test_case(fps: f64) {
            let mut sdl = MockSdl::default();
            sdl.expect_events().returning(|| Box::new([].into_iter()));
            let frame_duration = Duration::from_secs_f64(1_f64 / fps);
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            sdl.expect_fill_canvas().return_const(Ok(()));
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            Transition::FadeToBlack.play(&mut sdl, false).unwrap();

            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_secs(), 2);
        }
    }

    #[test]
    fn crossfade_play_takes_one_second_and_is_fps_independent() {
        test_case(30_f64);
        test_case(60_f64);

        fn test_case(fps: f64) {
            let mut sdl = MockSdl::default();
            sdl.expect_events().returning(|| Box::new([].into_iter()));
            let frame_duration = Duration::from_secs_f64(1_f64 / fps);
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            sdl.expect_set_texture_alpha().return_const(());
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            Transition::Crossfade.play(&mut sdl, false).unwrap();

            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_secs(), 1);
        }
    }

    #[test]
    fn fade_to_black_play_mutates_alpha() {
        let mut sdl = MockSdl::default();
        sdl.expect_events().returning(|| Box::new([].into_iter()));
        sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        let alpha_prefix = [0, 8, 17];
        for alpha in alpha_prefix {
            /* Check alpha value for first 3 calls to fill_canvas. */
            sdl.expect_fill_canvas()
                .once()
                .withf(move |color| *color == Color::RGBA(0, 0, 0, alpha))
                .return_const(Ok(()));
        }
        /* Set up calls between first and last 3 iterations */
        const EXPECTED_ITERATIONS: usize = 62;
        let alpha_postfix = [17, 9, 0];
        sdl.expect_fill_canvas()
            .times(EXPECTED_ITERATIONS - alpha_prefix.len() - alpha_postfix.len())
            .return_const(Ok(()));
        for alpha in alpha_postfix {
            /* Check alpha value for last 3 calls to fill_canvas. */
            sdl.expect_fill_canvas()
                .once()
                .withf(move |color| *color == Color::RGBA(0, 0, 0, alpha))
                .return_const(Ok(()));
        }
        sdl.expect_present_canvas()
            .returning(move || MockClock::advance(frame_duration));

        Transition::FadeToBlack.play(&mut sdl, false).unwrap();

        sdl.checkpoint();
    }

    #[test]
    fn crossfade_play_mutates_alpha() {
        let mut sdl = MockSdl::default();
        sdl.expect_events().returning(|| Box::new([].into_iter()));
        sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
        const FPS: f64 = 30_f64;
        let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
        let alpha_prefix: [u8; 3] = [0, 8, 17];
        for alpha in alpha_prefix {
            /* Check alpha value for first 3 calls to fill_canvas. */
            sdl.expect_set_texture_alpha()
                .once()
                .withf(move |a, i| a == &alpha && i == &TextureIndex::Next)
                .return_const(());
        }
        /* Set up calls between first and last 3 iterations */
        const EXPECTED_ITERATIONS: usize = 31;
        let alpha_postfix: [u8; 3] = [238, 246, 255];
        sdl.expect_set_texture_alpha()
            .times(EXPECTED_ITERATIONS - alpha_prefix.len() - alpha_postfix.len())
            .return_const(());
        for alpha in alpha_postfix {
            /* Check alpha value for last 3 calls to fill_canvas. */
            sdl.expect_set_texture_alpha()
                .once()
                .withf(move |a, i| a == &alpha && i == &TextureIndex::Next)
                .return_const(());
        }
        sdl.expect_present_canvas()
            .returning(move || MockClock::advance(frame_duration));

        Transition::Crossfade.play(&mut sdl, false).unwrap();

        sdl.checkpoint();
    }

    #[test]
    fn transition_play_does_not_copy_update_notification_to_canvas_when_show_update_notification_is_false(
    ) {
        test_case(Transition::Crossfade);
        test_case(Transition::FadeToBlack);
        test_case(Transition::None);

        fn test_case(sut: Transition) {
            let mut sdl = MockSdl::default();
            sdl.expect_events().returning(|| Box::new([].into_iter()));
            sdl.expect_set_texture_alpha().return_const(());
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            sdl.expect_fill_canvas().return_const(Ok(()));
            sdl.expect_copy_update_notification_to_canvas()
                .never()
                .return_const(Ok(()));
            const FPS: f64 = 30_f64;
            let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();
            const SHOW_UPDATE_NOTIFICATION: bool = false;

            let result = sut.play(&mut sdl, SHOW_UPDATE_NOTIFICATION);

            assert!(result.is_ok());
            sdl.checkpoint();
        }
    }

    #[test]
    fn transition_play_exits_on_quit_event() {
        test_case(Transition::Crossfade);
        test_case(Transition::FadeToBlack);

        fn test_case(sut: Transition) {
            let mut sdl = MockSdl::default();
            sdl.expect_events()
                .times(15)
                .returning(|| Box::new([].into_iter()));
            /* Quit event occuring after 15 frames */
            sdl.expect_events()
                .return_once(|| Box::new([Event::Quit { timestamp: 0 }].into_iter()));
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            const FPS: f64 = 30_f64;
            let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
            sdl.expect_set_texture_alpha().return_const(());
            sdl.expect_fill_canvas().return_const(Ok(()));
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            let result = sut.play(&mut sdl, false);

            assert!(result.is_ok());
            sdl.checkpoint();
            /* Check if Quit occured after roughly half second */
            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_millis(), 499);
        }
    }

    fn reset_clock() {
        MockClock::set_time(Duration::ZERO);
    }
}
