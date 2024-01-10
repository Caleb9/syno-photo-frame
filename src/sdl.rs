//! Rendering

pub(crate) use sdl2::{event::Event, pixels::Color};

use sdl2::render::BlendMode;
use sdl2::{
    pixels::PixelFormatEnum,
    render::{Canvas, Texture, TextureCreator},
    video::{DisplayMode, Window, WindowContext},
    EventPump, VideoSubsystem,
};

use crate::ErrorToString;

#[cfg_attr(test, mockall::automock)]
/// Isolates [sdl2::Sdl] context for testing
pub trait Sdl {
    /// Gets screen size
    fn size(&self) -> (u32, u32);
    fn update_texture(&mut self, pixel_data: &[u8], index: TextureIndex) -> Result<(), String>;
    fn set_texture_alpha(&mut self, alpha: u8, index: TextureIndex);
    fn copy_texture_to_canvas(&mut self, index: TextureIndex) -> Result<(), String>;
    fn swap_textures(&mut self);
    fn fill_canvas(&mut self, color: Color) -> Result<(), String>;
    fn present_canvas(&mut self);
    fn events<'a>(&'a mut self) -> Box<dyn Iterator<Item = Event> + 'a>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum TextureIndex {
    Current,
    Next,
}

impl<'a> Sdl for SdlWrapper<'a> {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn update_texture(&mut self, pixel_data: &[u8], index: TextureIndex) -> Result<(), String> {
        self.textures[self.texture_index(index)]
            .update(None, pixel_data, self.pitch)
            .map_err_to_string()
    }

    fn set_texture_alpha(&mut self, alpha: u8, index: TextureIndex) {
        self.textures[self.texture_index(index)].set_alpha_mod(alpha)
    }

    fn copy_texture_to_canvas(&mut self, index: TextureIndex) -> Result<(), String> {
        self.canvas
            .copy(&self.textures[self.texture_index(index)], None, None)
    }

    fn swap_textures(&mut self) {
        self.current_texture = (self.current_texture + 1) % self.textures.len();
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

/// Container for components from [sdl2::Sdl]
pub struct SdlWrapper<'a> {
    canvas: Canvas<Window>,
    textures: [Texture<'a>; 2],
    current_texture: usize,
    events: EventPump,
    size: (u32, u32),
    /// Number of bytes in a row of pixel data, in other words image width multiplied by bytes-per-pixel
    pitch: usize,
}

impl<'a> SdlWrapper<'a> {
    pub fn new(canvas: Canvas<Window>, textures: [Texture<'a>; 2], events: EventPump) -> Self {
        let (w, h) = canvas.window().size();
        const BYTE_SIZE_PER_PIXEL: usize = 3;
        SdlWrapper {
            canvas,
            textures,
            current_texture: 0,
            events,
            size: (w, h),
            pitch: (w as usize * BYTE_SIZE_PER_PIXEL),
        }
    }

    fn texture_index(&self, index: TextureIndex) -> usize {
        match index {
            TextureIndex::Current => self.current_texture,
            TextureIndex::Next => (self.current_texture + 1) % self.textures.len(),
        }
    }
}

/// Initializes SDL video subsystem. **Must be called before using any other function in this module**
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

/// Sets up a renderer
pub fn create_canvas(video: &VideoSubsystem, (w, h): (u32, u32)) -> Result<Canvas<Window>, String> {
    let window = video
        .window("syno-photo-frame", w, h)
        .fullscreen()
        .build()
        .map_err_to_string()?;
    /* Seems this needs to be set _after_ window has been created. */
    video.sdl().mouse().show_cursor(false);
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err_to_string()?;
    /* Transition effects draw semi-transparent box on canvas */
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    Ok(canvas)
}

/// Creates a texture which will contain rendered images
pub fn create_texture(
    texture_creator: &TextureCreator<WindowContext>,
    (w, h): (u32, u32),
) -> Result<Texture, String> {
    let mut texture = texture_creator
        .create_texture_static(PixelFormatEnum::RGB24, w, h)
        .map_err_to_string()?;
    texture.set_blend_mode(BlendMode::Blend);
    Ok(texture)
}
