use anyhow::{Context, Result};
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;
use tracing::{debug, warn};

use crate::track::TrackMeta;

pub fn write_tags(filepath: &Path, track: &TrackMeta) -> Result<()> {
    debug!("Writing tags to {:?}", filepath);

    let mut tagged_file = Probe::open(filepath)
        .with_context(|| format!("Failed to open {:?} for tagging", filepath))?
        .guess_file_type()
        .with_context(|| format!("Failed to guess file type for {:?}", filepath))?
        .read()
        .with_context(|| format!("Failed to read tagged file {:?}", filepath))?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => {
            // Insert a tag if there is none
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(lofty::tag::Tag::new(tag_type));
            tagged_file.primary_tag_mut().ok_or_else(|| {
                anyhow::anyhow!("Failed to create tag for {:?}", filepath)
            })?
        }
    };

    if !track.title.is_empty() {
        tag.set_title(track.title.clone());
    }
    if !track.artist.is_empty() {
        tag.set_artist(track.artist.clone());
    }
    if !track.album.is_empty() {
        tag.set_album(track.album.clone());
    }
    if !track.album_artist.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::AlbumArtist, track.album_artist.clone());
    }
    if !track.discogs_release_id.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(
            ItemKey::Unknown("DISCOGS_RELEASEID".to_string()),
            track.discogs_release_id.clone(),
        );
    }
    if !track.genre.is_empty() {
        tag.set_genre(track.genre.clone());
    }
    if !track.track_number.is_empty() {
        if let Ok(n) = track.track_number.parse::<u32>() {
            tag.set_track(n);
        }
    }
    if !track.year.is_empty() {
        if let Ok(y) = track.year.parse::<u32>() {
            tag.set_year(y);
        }
    }

    tagged_file.save_to_path(filepath, lofty::config::WriteOptions::default())
        .with_context(|| format!("Failed to save tags to {:?}", filepath))?;

    debug!("Tags written successfully to {:?}", filepath);
    Ok(())
}
