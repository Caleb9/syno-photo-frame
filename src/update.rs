use std::{
    sync::mpsc::SyncSender,
    thread::{Scope, ScopedJoinHandle},
};

use anyhow::Result;
use image::DynamicImage;

use crate::{
    api_crates, asset,
    cli::Rotation,
    http::HttpClient,
    img::Framed,
    sdl::{Sdl, TextureIndex},
};

pub struct UpdateNotification {
    pub is_visible: bool,
    icon: DynamicImage,
    rotation: Rotation,
}

impl UpdateNotification {
    pub fn new(screen_size: (u32, u32), rotation: Rotation) -> Result<Self> {
        Ok(UpdateNotification {
            is_visible: false,
            icon: asset::update_icon(screen_size, rotation)?,
            rotation,
        })
    }

    pub fn show_on_current_image(
        &self,
        current_image: &mut DynamicImage,
        sdl: &mut impl Sdl,
    ) -> Result<()> {
        self.overlay(current_image);
        sdl.update_texture(current_image.as_bytes(), TextureIndex::Current)?;
        sdl.copy_texture_to_canvas(TextureIndex::Current)?;
        sdl.present_canvas();
        Ok(())
    }

    pub fn overlay(&self, onto: &mut DynamicImage) {
        onto.overlay_update_icon(&self.icon, self.rotation);
    }
}

pub fn check_for_updates_thread<'a, C: HttpClient + Sync>(
    client: &'a C,
    this_crate_version: &'a str,
    thread_scope: &'a Scope<'a, '_>,
    update_check_sender: SyncSender<bool>,
) -> ScopedJoinHandle<'a, ()> {
    thread_scope.spawn(move || {
        match api_crates::get_latest_version(client) {
            Ok(remote_crate) => {
                if remote_crate.vers != this_crate_version {
                    log::info!(
                        "New version is available ({this_crate_version} -> {})",
                        remote_crate.vers
                    );
                    update_check_sender.try_send(true).unwrap_or_default();
                }
            }
            Err(error) => {
                log::error!("Check for updates: {error}");
            }
        };
    })
}
