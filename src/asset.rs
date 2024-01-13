use image::DynamicImage;

use crate::img;

pub(crate) fn welcome_image(size: (u32, u32)) -> Result<DynamicImage, String> {
    #[cfg(target_os = "linux")]
    const LOADING: &[u8] = include_bytes!("../assets/Loading.jpeg");
    #[cfg(target_os = "windows")]
    const LOADING: &[u8] = include_bytes!("..\\assets\\Loading.jpeg");
    load_and_resize(LOADING, size)
}

pub(crate) fn error_image(size: (u32, u32)) -> Result<DynamicImage, String> {
    #[cfg(target_os = "linux")]
    const ERROR_BYTES: &[u8] = include_bytes!("../assets/Error.jpeg");
    #[cfg(target_os = "windows")]
    const ERROR_BYTES: &[u8] = include_bytes!("..\\assets\\Error.jpeg");
    load_and_resize(ERROR_BYTES, size)
}

#[cfg(target_os = "linux")]
pub(crate) const FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
#[cfg(target_os = "windows")]
pub(crate) const FONT_BYTES: &[u8] = include_bytes!("..\\assets\\DejaVuSans.ttf");

fn load_and_resize(bytes: &[u8], (w, h): (u32, u32)) -> Result<DynamicImage, String> {
    Ok(img::load_from_memory(bytes)?.resize_exact(w, h, image::imageops::FilterType::Nearest))
}
