use std::{
    sync::mpsc::SyncSender,
    thread::{Scope, ScopedJoinHandle},
};

use image::DynamicImage;

use crate::{
    api_crates, asset,
    cli::Rotation,
    error::SynoPhotoFrameError,
    http::Client,
    img::Framed,
    sdl::{Sdl, TextureIndex},
};

pub(crate) struct UpdateNotification {
    pub is_visible: bool,
    icon: DynamicImage,
    rotation: Rotation,
}

impl UpdateNotification {
    pub(crate) fn new(screen_size: (u32, u32), rotation: Rotation) -> Result<Self, String> {
        Ok(UpdateNotification {
            is_visible: false,
            icon: asset::update_icon(screen_size, rotation)?,
            rotation,
        })
    }

    pub(crate) fn show_on_current_image(
        &self,
        current_image: &mut DynamicImage,
        sdl: &mut impl Sdl,
    ) -> Result<(), String> {
        self.overlay(current_image);
        sdl.update_texture(current_image.as_bytes(), TextureIndex::Current)?;
        sdl.copy_texture_to_canvas(TextureIndex::Current)?;
        sdl.present_canvas();
        Ok(())
    }

    pub(crate) fn overlay(&self, onto: &mut DynamicImage) {
        onto.overlay_update_icon(&self.icon, self.rotation);
    }
}

pub(crate) fn check_for_updates_thread<'a, C: Client + Clone + Send>(
    client: &'a C,
    installed_version: &'a str,
    thread_scope: &'a Scope<'a, '_>,
    update_available_tx: SyncSender<bool>,
) -> ScopedJoinHandle<'a, Result<(), SynoPhotoFrameError>> {
    let client = client.clone();
    thread_scope.spawn(move || {
        match api_crates::get_latest_version(&client) {
            Ok(remote_crate) => {
                if remote_crate.vers != installed_version {
                    log::info!(
                        "New version is available ({installed_version} -> {})",
                        remote_crate.vers
                    );
                    update_available_tx.try_send(true)?;
                }
            }
            Err(error) => {
                log::error!("Check for updates: {error}");
            }
        };
        Ok(())
    })
}
