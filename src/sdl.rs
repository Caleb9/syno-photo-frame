pub(crate) use sdl2::{event::Event, pixels::Color};

use sdl2::{
    pixels::PixelFormatEnum,
    render::{Canvas, Texture, TextureCreator},
    video::{DisplayMode, Window, WindowContext},
    EventPump, VideoSubsystem,
};

use crate::ErrorToString;

#[cfg_attr(test, mockall::automock)]
pub trait Sdl {
    /// Gets screen size
    fn size(&self) -> (u32, u32);
    fn update_texture(&mut self, pixel_data: &[u8]) -> Result<(), String>;
    fn copy_texture_to_canvas(&mut self) -> Result<(), String>;
    fn fill_canvas(&mut self, color: Color) -> Result<(), String>;
    fn present_canvas(&mut self);
    fn events<'a>(&'a mut self) -> Box<dyn Iterator<Item = Event> + 'a>;
}

impl<'a> Sdl for SdlWrapper<'a> {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn update_texture(&mut self, pixel_data: &[u8]) -> Result<(), String> {
        self.texture
            .update(None, pixel_data, self.pitch)
            .map_err_to_string()
    }

    fn copy_texture_to_canvas(&mut self) -> Result<(), String> {
        self.canvas.copy(&self.texture, None, None)
    }

    fn fill_canvas(&mut self, color: Color) -> Result<(), String> {
        self.canvas.set_draw_color(color);
        self.canvas.fill_rect(None)
    }

    fn present_canvas(&mut self) {
        self.canvas.present()
    }

    fn events(&mut self) -> Box<dyn Iterator<Item = Event> + '_> {
        Box::new(self.events.poll_iter())
    }
}

pub struct SdlWrapper<'a> {
    canvas: Canvas<Window>,
    texture: Texture<'a>,
    events: EventPump,
    size: (u32, u32),
    /// Number of bytes in a row of pixel data, in other words image width multiplied by bytes-per-pixel
    pitch: usize,
}

impl<'a> SdlWrapper<'a> {
    pub fn new(canvas: Canvas<Window>, texture: Texture<'a>, events: EventPump) -> Self {
        let (w, h) = canvas.window().size();
        const BYTE_SIZE_PER_PIXEL: usize = 3;
        SdlWrapper {
            canvas,
            texture,
            events,
            size: (w, h),
            pitch: (w as usize * BYTE_SIZE_PER_PIXEL),
        }
    }
}

pub fn init_video() -> Result<VideoSubsystem, String> {
    sdl2::init()?.video()
}

/// Returns screen width and height
pub fn display_size(video: &VideoSubsystem) -> Result<(u32, u32), String> {
    let DisplayMode {
        format: _, w, h, ..
    } = video.current_display_mode(0)?;
    Ok((u32::try_from(w).unwrap(), u32::try_from(h).unwrap()))
}

pub fn create_canvas(video: &VideoSubsystem, (w, h): (u32, u32)) -> Result<Canvas<Window>, String> {
    let window = video
        .window("syno-photo-frame", w, h)
        .fullscreen()
        .build()
        .map_err_to_string()?;
    /* Seems this needs to be set after window has been created to work. */
    video.sdl().mouse().show_cursor(false);
    let mut canvas = window.into_canvas().build().map_err_to_string()?;
    /* Transition effects draw semi-transparent box on canvas */
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
