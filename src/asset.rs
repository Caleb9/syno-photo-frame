use image::DynamicImage;

use crate::img;

pub(crate) fn welcome_image((w, h): (u32, u32)) -> Result<DynamicImage, String> {
    const LOADING: &[u8] = include_bytes!("../images/Loading.jpeg");
    Ok(img::load_from_memory(LOADING)?.resize(w, h, image::imageops::FilterType::Nearest))
}

pub(crate) fn error_image((w, h): (u32, u32)) -> Result<DynamicImage, String> {
    const ERROR_BYTES: &[u8] = include_bytes!("../images/Error.jpeg");
    Ok(img::load_from_memory(ERROR_BYTES)?.resize(w, h, image::imageops::FilterType::Nearest))
}
