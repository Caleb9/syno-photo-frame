use image::{DynamicImage, GenericImageView, Rgba};
use sdl2::{
    pixels::PixelFormatEnum,
    render::{Canvas, Texture, TextureCreator},
    video::{DisplayMode, Window, WindowContext},
    VideoSubsystem,
};

use super::ErrorToString;

pub fn init_video() -> Result<VideoSubsystem, String> {
    Ok(sdl2::init()?.video()?)
}

/// Returns (width, height, bpp)
pub fn dimensions(video_subsystem: &VideoSubsystem) -> Result<(u32, u32, usize), String> {
    let DisplayMode {
        format: _, w, h, ..
    } = video_subsystem.current_display_mode(0)?;
    Ok((
        u32::try_from(w).unwrap(),
        u32::try_from(h).unwrap(),
        3 as usize,
    ))
}

pub fn create_canvas(
    video_subsystem: &VideoSubsystem,
    w: u32,
    h: u32,
) -> Result<Canvas<Window>, String> {
    let window = video_subsystem
        .window("rust-sdl2 demo: Video", w, h)
        .fullscreen()
        .build()
        .map_err_to_string()?;
    /* Seems this needs to be set after window has been created to work. */
    video_subsystem.sdl().mouse().show_cursor(false);
    let mut canvas = window
        .into_canvas()
        .software()
        .build()
        .map_err_to_string()?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    Ok(canvas)
}

pub fn create_texture(
    texture_creator: &TextureCreator<WindowContext>,
    (w, h): (u32, u32),
) -> Result<Texture, String> {
    texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, w, h)
        .map_err_to_string()
}

pub fn image_to_texture(image: DynamicImage, bpp: usize) -> impl FnOnce(&mut [u8], usize) {
    move |buffer: &mut [u8], pitch: usize| {
        for (x, y, Rgba([r, g, b, ..])) in image.pixels() {
            let (x, y) = (usize::try_from(x).unwrap(), usize::try_from(y).unwrap());
            let offset = y * pitch + x * bpp;
            buffer[offset..=offset + 2].copy_from_slice(&[r, g, b]);
        }
    }
}
