use image::{
    self,
    imageops::{self, FilterType},
    DynamicImage,
};

type Dimensions = (f64, f64);

/// Prepares a frame by resizing and centering the original image and filling any empty space with blurred background.
pub fn prepare_photo_for_display(
    original: &DynamicImage,
    (xres, yres): (u32, u32),
) -> DynamicImage {
    let foreground = original.resize(xres, yres, FilterType::Lanczos3);
    let foreground_dimensions = (foreground.width() as f64, foreground.height() as f64);
    let (w_diff, h_diff) = dimensions_diff((xres as f64, yres as f64), foreground_dimensions);
    if w_diff as u32 == 0 && h_diff as u32 == 0 {
        // image fits perfectly, background not needed
        return foreground;
    }

    let (bg_fill1, bg_fill2) =
        background_crops_dimensions(foreground_dimensions, (xres as f64, yres as f64));

    let mut final_image = DynamicImage::new_rgb8(xres, yres);

    let bg_fill = background(&foreground, bg_fill1, (xres, yres));
    imageops::overlay(&mut final_image, &bg_fill, 0, 0);

    let bg_fill = background(&foreground, bg_fill2, (xres, yres));
    let (bg_fill_x, bg_fill_y) = (xres - bg_fill.width(), yres - bg_fill.height());
    imageops::overlay(
        &mut final_image,
        &bg_fill,
        bg_fill_x as i64,
        bg_fill_y as i64,
    );

    imageops::overlay(
        &mut final_image,
        &foreground,
        (w_diff / 2.0).round() as i64,
        (h_diff / 2.0).round() as i64,
    );
    final_image
}

fn background_crops_dimensions(
    foreground: Dimensions,
    screen: Dimensions,
) -> ((f64, f64, f64, f64), (f64, f64, f64, f64)) {
    let screen_to_image_projection = resize(screen, foreground);
    let (w_diff, h_diff) = dimensions_diff(screen_to_image_projection, foreground);
    let (bg_x, bg_y) = (w_diff / 2.0, h_diff / 2.0);

    let image_to_projected_screen = resize(foreground, screen_to_image_projection);
    let (w_diff, h_diff) = dimensions_diff(image_to_projected_screen, screen_to_image_projection);

    let (screen_w, screen_h) = screen_to_image_projection;

    if w_diff > 0.0 {
        /* Needs background on left and right.
         * The +1.0 is a one pixel overlap to workaround rounding errors */
        let bg_w = w_diff / 2.0 + 1.0;
        (
            (bg_x, bg_y, bg_w, screen_h),
            (foreground.0 - bg_w, bg_y, bg_w, screen_h),
        )
    } else {
        /* Needs background on top and bottom .*/
        let bg_h = h_diff / 2.0 + 1.0;
        (
            (bg_x, bg_y, screen_w, bg_h),
            (bg_x, foreground.1 - bg_h, screen_w, bg_h),
        )
    }
}

fn resize((width, height): Dimensions, (new_width, new_height): Dimensions) -> Dimensions {
    let wratio = new_width / width;
    let hratio = new_height / height;

    let ratio = f64::min(wratio, hratio);

    let nw = f64::max(width * ratio, 1.0);
    let nh = f64::max(height * ratio, 1.0);

    (nw, nh)
}

fn dimensions_diff((w1, h1): Dimensions, (w2, h2): Dimensions) -> (f64, f64) {
    (f64::abs(w1 - w2), f64::abs(h1 - h2))
}

fn background(
    foreground: &DynamicImage,
    (bg_fill_x, bg_fill_y, bg_fill_w, bg_fill_h): (f64, f64, f64, f64),
    (xres, yres): (u32, u32),
) -> DynamicImage {
    let (blur_sigma, brightness_offset) = (50.0, -30);
    foreground
        .crop_imm(
            bg_fill_x.floor() as u32,
            bg_fill_y.floor() as u32,
            bg_fill_w.ceil() as u32,
            bg_fill_h.ceil() as u32,
        )
        .brighten(brightness_offset)
        .resize(xres, yres, FilterType::Nearest)
        .blur(blur_sigma)
}
