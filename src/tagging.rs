use anyhow::{Context, Result};
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;
use tracing::debug;

use crate::track::TrackMeta;

/// Write all metadata tags to an exported audio file.
///
/// `effective_comments` is the comment to embed — callers should resolve
/// this as: `track.comments` if non-empty, else `config.default_comments`.
pub fn write_tags(filepath: &Path, track: &TrackMeta, effective_comments: &str) -> Result<()> {
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
    if !track.composer.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::Composer, track.composer.clone());
    }
    if !effective_comments.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::Comment, effective_comments.to_string());
    }
    if !track.country.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::Unknown("COUNTRY".to_string()), track.country.clone());
    }
    if !track.label.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::Unknown("ORGANIZATION".to_string()), track.label.clone());
    }
    if !track.catalog.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(ItemKey::Unknown("CATALOGNUMBER".to_string()), track.catalog.clone());
    }

    tagged_file.save_to_path(filepath, lofty::config::WriteOptions::default())
        .with_context(|| format!("Failed to save tags to {:?}", filepath))?;

    debug!("Tags written successfully to {:?}", filepath);
    Ok(())
}
