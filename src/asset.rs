use crate::{
    cli::Rotation,
    img::{self, DynamicImage, Framed},
};

type Result = core::result::Result<DynamicImage, String>;

pub(crate) fn welcome_screen(screen_size: (u32, u32), rotation: Rotation) -> Result {
    #[cfg(not(target_os = "windows"))]
    const LOADING: &[u8] = include_bytes!("../assets/Loading.jpeg");
    #[cfg(target_os = "windows")]
    const LOADING: &[u8] = include_bytes!("..\\assets\\Loading.jpeg");
    load_and_resize(LOADING, screen_size, rotation)
}

pub(crate) fn error_screen(screen_size: (u32, u32), rotation: Rotation) -> Result {
    #[cfg(not(target_os = "windows"))]
    const ERROR_BYTES: &[u8] = include_bytes!("../assets/Error.jpeg");
    #[cfg(target_os = "windows")]
    const ERROR_BYTES: &[u8] = include_bytes!("..\\assets\\Error.jpeg");
    load_and_resize(ERROR_BYTES, screen_size, rotation)
}

pub(crate) fn update_icon((screen_width, _): (u32, u32), rotation: Rotation) -> Result {
    #[cfg(not(target_os = "windows"))]
    const UPDATE_BYTES: &[u8] = include_bytes!("../assets/Update.png");
    #[cfg(target_os = "windows")]
    const UPDATE_BYTES: &[u8] = include_bytes!("..\\assets\\Update.png");

    /* Resize the update icon to 1/15th of the screen width */
    let (icon_w, icon_h) = (screen_width / 15, screen_width / 15);
    Ok(Framed::resize(&img::load_from_memory(UPDATE_BYTES)?, icon_w, icon_h).rotate(rotation))
}

fn load_and_resize(bytes: &[u8], screen_size: (u32, u32), rotation: Rotation) -> Result {
    Ok(img::load_from_memory(bytes)?.fit_to_screen(screen_size, rotation))
}
