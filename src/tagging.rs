/*
 *  tagging.rs
 *
 *  vripr - The vinyl viper for perfect rippage - Audacity vinyl ripping helper
 *	(c) 2025-26 Stuart Hunter
 *
 *	TODO:
 *
 * MIT License
 * 
 * Copyright (c) 2026 VRipr Contributors
 * 
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 * 
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 * 
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 *
 */

#[allow(dead_code)]
#[allow(unused_imports)]
use anyhow::{Context, Result};
use lofty::prelude::*;
use lofty::probe::Probe;
use std::path::Path;
use tracing::debug;

use crate::metadata::sanitize_genres;
use crate::track::TrackMeta;

/// Split a semicolon-delimited artist string into individual, trimmed artist names.
/// Filters out empty strings so a trailing semicolon doesn't produce a blank entry.
fn split_artists(s: &str) -> Vec<String> {
    s.split(';')
     .map(|a| a.trim().to_string())
     .filter(|a| !a.is_empty())
     .collect()
}

/// Write all metadata tags to an exported audio file.
///
/// `effective_comments` is the comment to embed — callers should resolve
/// this as: `track.comments` if non-empty, else `config.default_comments`.
///
/// `extra_tags` is a slice of `(name, value)` pairs written as freeform tags
/// using `ItemKey::Unknown`. Pairs with an empty name are silently skipped.
pub fn write_tags(
    filepath: &Path,
    track: &TrackMeta,
    effective_comments: &str,
    extra_tags: &[(String, String)],
) -> Result<()> {
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
    // Write multi-value ARTIST tags — semicolons delimit multiple contributors
    // (e.g. "Daniel Mana;Mana" → two separate ARTIST tags).
    let artists = split_artists(&track.artist);
    if !artists.is_empty() {
        use lofty::tag::{ItemKey, ItemValue, TagItem};
        tag.remove_key(&ItemKey::TrackArtist);
        for a in &artists {
            tag.push(TagItem::new(ItemKey::TrackArtist, ItemValue::Text(a.clone())));
        }
    }
    if !track.album.is_empty() {
        tag.set_album(track.album.clone());
    }
    // Write multi-value ALBUMARTIST tags with the same logic.
    let album_artists = split_artists(&track.album_artist);
    if !album_artists.is_empty() {
        use lofty::tag::{ItemKey, ItemValue, TagItem};
        tag.remove_key(&ItemKey::AlbumArtist);
        for a in &album_artists {
            tag.push(TagItem::new(ItemKey::AlbumArtist, ItemValue::Text(a.clone())));
        }
    }
    if !track.discogs_release_id.is_empty() {
        use lofty::tag::ItemKey;
        tag.insert_text(
            ItemKey::Unknown("DISCOGS_RELEASEID".to_string()),
            track.discogs_release_id.clone(),
        );
    }
    // Sanitize and expand the semicolon-delimited genre string, then write
    // one tag item per genre so players that support multi-value tags see all of them.
    let genres = sanitize_genres(&track.genre);
    if !genres.is_empty() {
        use lofty::tag::{ItemKey, ItemValue, TagItem};
        tag.remove_key(&ItemKey::Genre);
        for g in &genres {
            tag.push(TagItem::new(ItemKey::Genre, ItemValue::Text(g.clone())));
        }
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
    for (name, value) in extra_tags {
        if !name.is_empty() {
            use lofty::tag::ItemKey;
            tag.insert_text(ItemKey::Unknown(name.clone()), value.clone());
        }
    }

    tagged_file.save_to_path(filepath, lofty::config::WriteOptions::default())
        .with_context(|| format!("Failed to save tags to {:?}", filepath))?;

    debug!("Tags written successfully to {:?}", filepath);
    Ok(())
}
