use anyhow::Result;

use crate::{cli::Rotation, sdl::Rect};

pub fn get_font_point_size((screen_w, screen_h): (u32, u32)) -> u16 {
    /* The minimum font size calculated as follows:
     * - we want the point size to be roughly 1/30 of the screen's smaller dimension
     * - we assume support for minimum screen resolution of 800x600, so the smaller dimension is 600
     * - 600 / 30 = 20
     * This may not look good on very low resolution screen, but we avoid setting the font to
     * unreasonably small size, where it would be unreadable. */
    const MIN_SIZE: u32 = 20;
    let screen_size = screen_w.min(screen_h);
    const SCREEN_SIZE_DIVISOR: u32 = 30;
    MIN_SIZE
        .max(screen_size / SCREEN_SIZE_DIVISOR)
        .try_into()
        .unwrap_or(u16::MAX)
}

pub fn get_stroke_font_outline_width((screen_w, screen_h): (u32, u32)) -> u16 {
    const MIN_WIDTH: u32 = 1;
    let screen_size = screen_w.min(screen_h);
    const STROKE_FONT_POINT_SIZE_DIVISOR: u32 = 480;
    let outline_width = screen_size
        .div_ceil(STROKE_FONT_POINT_SIZE_DIVISOR)
        .max(MIN_WIDTH);
    outline_width.try_into().unwrap_or(u16::MAX)
}

/// Calculate destination rectangle for photo info box on the bottom-left part of the screen
pub fn get_text_box_dst_rect(
    (screen_w, screen_h): (u32, u32),
    (text_surface_w, text_surface_h): (u32, u32),
    rotation: Rotation,
) -> Result<Rect> {
    let screen_size_min = screen_w.min(screen_h);
    const PADDING_SCREEN_SIZE_DIVISOR: u32 = 72;
    let padding = screen_size_min / PADDING_SCREEN_SIZE_DIVISOR;
    let (text_surface_w, text_surface_h) = scale_to_screen(
        (screen_w, screen_h),
        (text_surface_w, text_surface_h),
        padding,
    );
    let (rotated_x, rotated_y) = (
        screen_w
            .saturating_sub(text_surface_w)
            .saturating_sub(padding),
        screen_h
            .saturating_sub(text_surface_h)
            .saturating_sub(padding),
    );
    let (x, y) = match rotation {
        Rotation::D0 => (padding, rotated_y),
        Rotation::D90 => (padding, padding),
        Rotation::D180 => (rotated_x, padding),
        Rotation::D270 => (rotated_x, rotated_y),
    };
    Ok(Rect::new(
        x.min(i32::MAX as u32) as i32,
        y.min(i32::MAX as u32) as i32,
        text_surface_w,
        text_surface_h,
    ))
}

/// Scale text to guarantee it will fit on the screen.
///
/// When the text is not fitting on the screen, it will be scaled as bitmap with SDL. This results
/// in quite ugly looking, aliased fonts, but the hope is that the initial font size adjustment, and
/// the amount of text to be rendered is going to make this situation rare.
fn scale_to_screen(
    (screen_w, screen_h): (u32, u32),
    (text_w, text_h): (u32, u32),
    padding: u32,
) -> (u32, u32) {
    // guard for degenerate input
    if text_w == 0 || text_h == 0 {
        return (0, 0);
    }

    let (available_w, available_h) = (
        screen_w.saturating_sub(padding.saturating_mul(2)) as f64,
        screen_h.saturating_sub(padding.saturating_mul(2)) as f64,
    );
    let (text_w_f, text_h_f) = (text_w as f64, text_h as f64);

    let (scale_w, scale_h) = (available_w / text_w_f, available_h / text_h_f);
    let scale = scale_w.min(scale_h).clamp(0.0, 1.0);

    let (new_w, new_h) = (
        (text_w_f * scale).round() as u32,
        (text_h_f * scale).round() as u32,
    );

    // ensure at least 1 pixel if original was non-zero
    (new_w.max(1), new_h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_font_point_size_is_30th_of_smaller_screen_dimension() {
        let result = get_font_point_size((1920, 1080));
        assert_eq!(result, 36)
    }

    #[test]
    fn get_font_point_size_is_at_least_20() {
        let result = get_font_point_size((320, 240));
        assert_eq!(result, 20)
    }

    #[test]
    fn get_font_point_size_is_at_most_65535() {
        let result = get_font_point_size((u32::MAX, u32::MAX));
        assert_eq!(result, 65535)
    }

    #[test]
    fn get_stroke_font_outline_width_is_480th_of_screen_size() {
        let result = get_stroke_font_outline_width((1920, 1080));
        assert_eq!(result, 3)
    }

    #[test]
    fn get_stroke_font_outline_width_is_at_least_1() {
        let result = get_stroke_font_outline_width((320, 240));
        assert_eq!(result, 1)
    }

    #[test]
    fn get_stroke_font_outline_width_is_at_most_65535() {
        let result = get_stroke_font_outline_width((u32::MAX, u32::MAX));
        assert_eq!(result, 65535)
    }

    #[test]
    fn when_rotation_0_then_get_text_box_dst_rect_is_bottom_left_with_padding() {
        const SCREEN_SIZE: (u32, u32) = (1920, 1080);
        const TEXT_SURFACE_SIZE: (u32, u32) = (400, 100);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(15, 965, 400, 100))
    }

    #[test]
    fn when_rotation_90_then_get_text_box_dst_rect_is_top_left_with_padding() {
        const SCREEN_SIZE: (u32, u32) = (1920, 1080);
        const TEXT_SURFACE_SIZE: (u32, u32) = (100, 400);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D90);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(15, 15, 100, 400))
    }

    #[test]
    fn when_rotation_180_then_get_text_box_dst_rect_is_top_right_with_padding() {
        const SCREEN_SIZE: (u32, u32) = (1920, 1080);
        const TEXT_SURFACE_SIZE: (u32, u32) = (400, 100);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D180);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(1505, 15, 400, 100))
    }

    #[test]
    fn when_rotation_270_then_get_text_box_dst_rect_is_bottom_right_with_padding() {
        const SCREEN_SIZE: (u32, u32) = (1920, 1080);
        const TEXT_SURFACE_SIZE: (u32, u32) = (100, 400);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D270);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(1805, 665, 100, 400))
    }

    #[test]
    fn when_text_size_is_wider_than_screen_then_get_text_box_dst_rect_scales_down() {
        const SCREEN_SIZE: (u32, u32) = (800, 600);
        const TEXT_SURFACE_SIZE: (u32, u32) = (1200, 100);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(8, 527, 784, 65))
    }

    #[test]
    fn when_text_size_is_taller_than_screen_then_get_text_box_dst_rect_scales_down() {
        const SCREEN_SIZE: (u32, u32) = (800, 600);
        const TEXT_SURFACE_SIZE: (u32, u32) = (600, 1000);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(8, 8, 350, 584))
    }

    #[test]
    fn when_text_size_is_wider_and_taller_than_screen_then_get_text_box_dst_rect_scales_down() {
        const SCREEN_SIZE: (u32, u32) = (800, 600);
        const TEXT_SURFACE_SIZE: (u32, u32) = (1200, 750);

        let result = get_text_box_dst_rect(SCREEN_SIZE, TEXT_SURFACE_SIZE, Rotation::D0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Rect::new(8, 102, 784, 490))
    }

    #[test]
    fn scale_to_screen_with_zero_dimensions_returns_zero_zero() {
        assert_eq!(scale_to_screen((800, 600), (0, 100), 8), (0, 0));
        assert_eq!(scale_to_screen((800, 600), (100, 0), 8), (0, 0));
        assert_eq!(scale_to_screen((800, 600), (0, 0), 8), (0, 0));
    }

    #[test]
    fn scale_to_screen_when_available_area_is_zero_returns_minimum_one() {
        // available area -> screen 10 with padding 10 => saturating_sub -> 0
        assert_eq!(scale_to_screen((10, 10), (100, 100), 10), (1, 1));
    }

    #[test]
    fn scale_to_screen_scales_down_by_available_dimension() {
        // matches the behavior exercised through get_text_box_dst_rect tests:
        // available_w = 800 - 16 = 784, so 1200 * (784/1200) = 784, 100 * (...) = 65
        assert_eq!(scale_to_screen((800, 600), (1200, 100), 8), (784, 65));
    }

    #[test]
    fn get_text_box_dst_rect_with_zero_sized_screen_and_large_text_returns_1x1() {
        // screen is zero; padding is 0; scale_to_screen will return (1,1)
        let result = get_text_box_dst_rect((0, 0), (100, 100), Rotation::D0).unwrap();
        assert_eq!(result, Rect::new(0, 0, 1, 1));
    }

    #[test]
    fn get_text_box_dst_rect_with_zero_sized_text_returns_zero_sized_rect() {
        // text surface (0,0) should be preserved; padding = 600/72 = 8 for 800x600
        const SCREEN: (u32, u32) = (800, 600);

        let result = get_text_box_dst_rect(SCREEN, (0, 0), Rotation::D0).unwrap();

        // x = padding = 8, y = screen_h - 0 - padding = 592
        assert_eq!(result, Rect::new(8, 592, 0, 0));
    }
}
