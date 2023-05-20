#[cfg(test)]
use mock_instant::Instant;

#[cfg(not(test))]
use std::time::Instant;

use crate::sdl::{Color, Sdl};

#[derive(Debug)]
pub(crate) enum Transition {
    In,
    Out,
}

/// Possibly parametrize this and take command line argument to control length of the transition
const TRANSITION_DURATION_SECS: f64 = 2_f64;
const TRANSITION_ALPHA_MIN: f64 = 0_f64;
const TRANSITION_ALPHA_MAX: f64 = 255_f64;

impl Transition {
    pub(crate) fn play(&self, sdl: &mut impl Sdl) -> Result<(), String> {
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
            sdl.fill_canvas(Color::RGBA(0, 0, 0, alpha.round() as u8))?;
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
            Transition::In => alpha.round() <= TRANSITION_ALPHA_MIN,
            Transition::Out => alpha.round() >= TRANSITION_ALPHA_MAX,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mock_instant::MockClock;
    use mockall::Sequence;

    use crate::sdl::{Event, MockSdl};

    use super::*;

    #[test]
    fn transition_play_calls_canvas_methods_in_sequence() {
        test_case(Transition::In);
        test_case(Transition::Out);

        fn test_case(sut: Transition) {
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
                    .once()
                    .in_sequence(&mut canvas_seq)
                    .return_const(Ok(()));
                sdl.expect_fill_canvas()
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

            let result = sut.play(&mut sdl);

            assert!(result.is_ok());
            sdl.checkpoint();
        }
    }

    #[test]
    fn transition_play_takes_one_second_and_is_fps_independent() {
        test_case(Transition::In, 30_f64);
        test_case(Transition::Out, 30_f64);
        test_case(Transition::In, 60_f64);
        test_case(Transition::Out, 60_f64);

        fn test_case(sut: Transition, fps: f64) {
            let mut sdl = MockSdl::default();
            sdl.expect_events().returning(|| Box::new([].into_iter()));
            let frame_duration = Duration::from_secs_f64(1_f64 / fps);
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            sdl.expect_fill_canvas().return_const(Ok(()));
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            sut.play(&mut sdl).unwrap();

            let fade_duration = MockClock::time();
            assert_eq!(fade_duration.as_secs(), 1);
        }
    }

    #[test]
    fn transition_play_mutates_alpha() {
        test_case(Transition::In, [255, 247, 238], [17, 9, 0]);
        test_case(Transition::Out, [0, 8, 17], [238, 246, 255]);

        fn test_case(sut: Transition, alpha_prefix: [u8; 3], alpha_postfix: [u8; 3]) {
            let mut sdl = MockSdl::default();
            sdl.expect_events().returning(|| Box::new([].into_iter()));
            sdl.expect_copy_texture_to_canvas().return_const(Ok(()));
            const FPS: f64 = 30_f64;
            let frame_duration = Duration::from_secs_f64(1_f64 / FPS);
            for alpha in alpha_prefix {
                /* Check alpha value for first 3 calls to fill_canvas. */
                sdl.expect_fill_canvas()
                    .once()
                    .withf(move |color| *color == Color::RGBA(0, 0, 0, alpha))
                    .return_const(Ok(()));
            }
            /* Set up calls between first and last 3 iterations */
            const EXPECTED_ITERATIONS: usize = 31;
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

            sut.play(&mut sdl).unwrap();

            sdl.checkpoint();
        }
    }

    #[test]
    fn transition_play_exits_on_quit_event() {
        test_case(Transition::In);
        test_case(Transition::Out);

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
            sdl.expect_fill_canvas().return_const(Ok(()));
            sdl.expect_present_canvas()
                .returning(move || MockClock::advance(frame_duration));
            reset_clock();

            let result = sut.play(&mut sdl);

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
