use image::DynamicImage;

use crate::img;

pub(crate) fn welcome_image(size: (u32, u32)) -> Result<DynamicImage, String> {
    #[cfg(target_os = "linux")]
    const LOADING: &[u8] = include_bytes!("../images/Loading.jpeg");
    #[cfg(target_os = "windows")]
    const LOADING: &[u8] = include_bytes!("..\\images\\Loading.jpeg");
    load_and_resize(LOADING, size)
}

pub(crate) fn error_image(size: (u32, u32)) -> Result<DynamicImage, String> {
    #[cfg(target_os = "linux")]
    const ERROR_BYTES: &[u8] = include_bytes!("../images/Error.jpeg");
    #[cfg(target_os = "windows")]
    const ERROR_BYTES: &[u8] = include_bytes!("..\\images\\Error.jpeg");
    load_and_resize(ERROR_BYTES, size)
}

fn load_and_resize(bytes: &[u8], (w, h): (u32, u32)) -> Result<DynamicImage, String> {
    Ok(img::load_from_memory(bytes)?.resize_exact(w, h, image::imageops::FilterType::Nearest))
}
