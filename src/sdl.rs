//! Rendering

pub(crate) use sdl2::{pixels::Color, rect::Rect};

use anyhow::Result;
use sdl2::{
    EventPump, VideoSubsystem,
    event::Event,
    gfx::rotozoom::RotozoomSurface,
    pixels::PixelFormatEnum,
    render::{BlendMode, Canvas, Texture, TextureCreator},
    rwops::RWops,
    video::{DisplayMode, Window, WindowContext},
};

use crate::{QuitEvent, cli::Rotation, error::AnyhowErrorMapper, info_box};

/// Isolates [sdl2::Sdl] context for testing
#[cfg_attr(test, mockall::automock)]
pub trait Sdl {
    /// Gets screen size
    fn size(&self) -> (u32, u32);
    fn update_texture(&mut self, image_data: &[u8], index: TextureIndex) -> Result<()>;
    fn set_texture_alpha(&mut self, alpha: u8, index: TextureIndex);
    fn clear_canvas(&mut self);
    fn copy_texture_to_canvas(&mut self, index: TextureIndex) -> Result<()>;
    /// Swaps current texture with the next one
    fn swap_textures(&mut self);
    fn fill_canvas(&mut self, color: Color) -> Result<()>;
    fn present_canvas(&mut self);
    fn handle_quit_event(&mut self) -> Result<(), QuitEvent>;

    /// Renders text on the canvas. Used for shooting info box.
    fn render_info_box(&mut self, text: &str, rotation: Rotation) -> Result<()>;
}

/// Index of a texture to operate on (used mainly by transition effects)
#[derive(Debug, PartialEq, Eq)]
pub enum TextureIndex {
    /// Currently active texture containing displayed image
    Current,
    /// Texture containing the next image to display
    Next,
}

/// Container for components from [sdl2::Sdl]
pub struct SdlWrapper<'a> {
    canvas: Canvas<Window>,
    texture_creator: &'a TextureCreator<WindowContext>,
    textures: [Texture<'a>; 2],
    current_texture: usize,
    size: (u32, u32),
    /// Number of bytes in a row of pixel data. In other words: image width multiplied by
    /// bytes-per-pixel
    pitch: usize,
    fonts: Fonts<'a>,
    events: EventPump,
}

struct Fonts<'a> {
    /// Foreground font
    fill: sdl2::ttf::Font<'a, 'a>,
    /// Border / outline font
    stroke: sdl2::ttf::Font<'a, 'a>,
    stroke_outline_width: u16,
}

impl Sdl for SdlWrapper<'_> {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn update_texture(&mut self, image_data: &[u8], index: TextureIndex) -> Result<()> {
        Ok(self.textures[self.texture_index(index)].update(None, image_data, self.pitch)?)
    }

    fn set_texture_alpha(&mut self, alpha: u8, index: TextureIndex) {
        self.textures[self.texture_index(index)].set_alpha_mod(alpha)
    }

    fn clear_canvas(&mut self) {
        self.canvas.clear()
    }

    fn copy_texture_to_canvas(&mut self, index: TextureIndex) -> Result<()> {
        self.canvas
            .copy(&self.textures[self.texture_index(index)], None, None)
            .map_err_to_anyhow()
    }

    fn swap_textures(&mut self) {
        self.current_texture = (self.current_texture + 1) % self.textures.len();
    }

    fn fill_canvas(&mut self, color: Color) -> Result<()> {
        self.canvas.set_draw_color(color);
        self.canvas.fill_rect(None).map_err_to_anyhow()
    }

    fn present_canvas(&mut self) {
        self.canvas.present()
    }

    fn handle_quit_event(&mut self) -> Result<(), QuitEvent> {
        let exit_requested = self.events.poll_iter().any(|e| {
            if let event @ (Event::Quit { .. } | Event::AppTerminating { .. }) = e {
                log::debug!("SDL event received: {event:?}");
                true
            } else {
                false
            }
        });
        if exit_requested {
            Err(QuitEvent)
        } else {
            Ok(())
        }
    }

    // TODO: there's too much logic here for the thin SDL wrapper. Refactor this.
    fn render_info_box(&mut self, text: &str, rotation: Rotation) -> Result<()> {
        const ORANGE: Color = Color::RGB(255, 170, 0);
        let fill_surface = self.fonts.fill.render(text).blended(ORANGE)?;
        let (w, h) = fill_surface.size();
        let dst_rect = Rect::new(
            self.fonts.stroke_outline_width as i32,
            self.fonts.stroke_outline_width as i32,
            w,
            h,
        );
        let mut stroke_surface = self.fonts.stroke.render(text).blended(Color::BLACK)?;
        fill_surface
            .blit(None, &mut stroke_surface, dst_rect)
            .map_err_to_anyhow()?;
        let turns = match rotation {
            Rotation::D0 => 0,
            Rotation::D90 => 1,
            Rotation::D180 => 2,
            Rotation::D270 => 3,
        };
        let rotated_surface = stroke_surface.rotate_90deg(turns).map_err_to_anyhow()?;
        let info_box_texture = self
            .texture_creator
            .create_texture_from_surface(&rotated_surface)?;
        let bottom_left =
            info_box::get_text_box_dst_rect(self.size, rotated_surface.size(), rotation)?;
        self.canvas
            .copy(&info_box_texture, None, bottom_left)
            .map_err_to_anyhow()
    }
}

impl<'a> SdlWrapper<'a> {
    pub fn new(
        canvas: Canvas<Window>,
        texture_creator: &'a TextureCreator<WindowContext>,
        events: EventPump,
        ttf: &'a sdl2::ttf::Sdl2TtfContext,
    ) -> Result<Self> {
        let screen_size = canvas.window().size();
        let (w, _) = screen_size;
        const BYTE_SIZE_PER_PIXEL: usize = 3;
        let stroke_font_outline_width = info_box::get_stroke_font_outline_width(screen_size);
        let sdl_wrapper = SdlWrapper {
            canvas,
            texture_creator,
            textures: [
                create_texture(texture_creator, screen_size)?,
                create_texture(texture_creator, screen_size)?,
            ],
            current_texture: 0,
            size: screen_size,
            pitch: w as usize * BYTE_SIZE_PER_PIXEL,
            fonts: Fonts {
                fill: load_font(ttf, screen_size, None)?,
                stroke: load_font(ttf, screen_size, Some(stroke_font_outline_width))?,
                stroke_outline_width: stroke_font_outline_width,
            },
            events,
        };
        Ok(sdl_wrapper)
    }

    fn texture_index(&self, index: TextureIndex) -> usize {
        match index {
            TextureIndex::Current => self.current_texture,
            TextureIndex::Next => (self.current_texture + 1) % self.textures.len(),
        }
    }
}

/// Initializes SDL video subsystem.
/// **Must be called before using any other function in this module**
pub fn init() -> Result<sdl2::Sdl> {
    sdl2::init().map_err_to_anyhow()
}

pub fn init_ttf() -> Result<sdl2::ttf::Sdl2TtfContext> {
    sdl2::ttf::init().map_err_to_anyhow()
}

/// Returns screen width and height
pub fn display_size(video: &VideoSubsystem) -> Result<(u32, u32)> {
    let DisplayMode {
        format: _, w, h, ..
    } = video.current_display_mode(0).map_err_to_anyhow()?;
    Ok((u32::try_from(w)?, u32::try_from(h)?))
}

/// Sets up a renderer
pub fn create_canvas(video: &VideoSubsystem, (w, h): (u32, u32)) -> Result<Canvas<Window>> {
    let window = video
        .window("syno-photo-frame", w, h)
        .borderless()
        .build()?;
    /* Seems this needs to be set _after_ window has been created. */
    video.sdl().mouse().show_cursor(false);
    let mut canvas = window.into_canvas().present_vsync().build()?;
    /* Transition effects draw semi-transparent box on canvas */
    canvas.set_blend_mode(BlendMode::Blend);
    Ok(canvas)
}

/// Creates a texture which will contain rendered images
fn create_texture(
    texture_creator: &TextureCreator<WindowContext>,
    (w, h): (u32, u32),
) -> Result<Texture<'_>> {
    let mut texture = texture_creator.create_texture_static(PixelFormatEnum::RGB24, w, h)?;
    texture.set_blend_mode(BlendMode::Blend);
    Ok(texture)
}

fn load_font(
    ttf: &'_ sdl2::ttf::Sdl2TtfContext,
    screen_size: (u32, u32),
    outline_width: Option<u16>,
) -> Result<sdl2::ttf::Font<'_, '_>> {
    let mut font = ttf
        .load_font_from_rwops(
            RWops::from_bytes(crate::asset::FONT_BYTES).map_err_to_anyhow()?,
            info_box::get_font_point_size(screen_size),
        )
        .map_err_to_anyhow()?;
    if let Some(outline_width) = outline_width {
        font.set_outline_width(outline_width);
    }
    Ok(font)
}
