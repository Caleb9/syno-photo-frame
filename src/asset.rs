use crate::{
    cli::Rotation,
    img::{self, DynamicImage, Framed},
};

pub fn welcome_screen(screen_size: (u32, u32), rotation: Rotation) -> Result<DynamicImage, String> {
    #[cfg(not(target_os = "windows"))]
    const LOADING: &[u8] = include_bytes!("../assets/Loading.jpeg");
    #[cfg(target_os = "windows")]
    const LOADING: &[u8] = include_bytes!("..\\assets\\Loading.jpeg");
    load_and_resize(LOADING, screen_size, rotation)
}

pub fn error_screen(screen_size: (u32, u32), rotation: Rotation) -> Result<DynamicImage, String> {
    #[cfg(not(target_os = "windows"))]
    const ERROR_BYTES: &[u8] = include_bytes!("../assets/Error.jpeg");
    #[cfg(target_os = "windows")]
    const ERROR_BYTES: &[u8] = include_bytes!("..\\assets\\Error.jpeg");
    load_and_resize(ERROR_BYTES, screen_size, rotation)
}

fn load_and_resize(
    bytes: &[u8],
    screen_size: (u32, u32),
    rotation: Rotation,
) -> Result<DynamicImage, String> {
    Ok(img::load_from_memory(bytes)?.fit_to_screen(screen_size, rotation))
}
