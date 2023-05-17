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
    (w, h): (u32, u32),
) -> Result<Canvas<Window>, String> {
    let window = video_subsystem
        .window("syno-photo-frame", w, h)
        .fullscreen()
        .build()
        .map_err_to_string()?;
    /* Seems this needs to be set after window has been created to work. */
    video_subsystem.sdl().mouse().show_cursor(false);
    let mut canvas = window.into_canvas().build().map_err_to_string()?;
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    Ok(canvas)
}

pub fn create_texture(
    texture_creator: &TextureCreator<WindowContext>,
    (w, h): (u32, u32),
) -> Result<Texture, String> {
    texture_creator
        .create_texture_static(PixelFormatEnum::RGB24, w, h)
        .map_err_to_string()
}
