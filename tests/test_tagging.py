"""Tests for apply_tags — verifies mutagen writes correct tag data."""
import pytest
from unittest.mock import patch, MagicMock
from vripr.app import TrackMeta, apply_tags


SAMPLE_TRACK = TrackMeta(
    index=1, start=0.0, end=180.0,
    title="Come Together",
    artist="The Beatles",
    album="Abbey Road",
    album_artist="The Beatles",
    genre="Rock",
    track_number="1",
    year="1969",
)


class TestApplyTags:
    def test_skipped_without_mutagen(self, tmp_path):
        p = tmp_path / "track.flac"
        p.write_bytes(b"")
        with patch("vripr.app.HAS_MUTAGEN", False):
            # should not raise
            apply_tags(str(p), SAMPLE_TRACK)

    def test_flac_tags_written(self, tmp_path):
        p = tmp_path / "track.flac"
        p.write_bytes(b"")
        mock_audio = MagicMock()
        with patch("vripr.app.HAS_MUTAGEN", True), \
             patch("vripr.app.FLAC", return_value=mock_audio):
            apply_tags(str(p), SAMPLE_TRACK)
        mock_audio.__setitem__.assert_any_call("title",       "Come Together")
        mock_audio.__setitem__.assert_any_call("artist",      "The Beatles")
        mock_audio.__setitem__.assert_any_call("album",       "Abbey Road")
        mock_audio.__setitem__.assert_any_call("albumartist", "The Beatles")
        mock_audio.__setitem__.assert_any_call("genre",       "Rock")
        mock_audio.__setitem__.assert_any_call("tracknumber", "1")
        mock_audio.__setitem__.assert_any_call("date",        "1969")
        mock_audio.save.assert_called_once()

    def test_mp3_tags_written(self, tmp_path):
        p = tmp_path / "track.mp3"
        p.write_bytes(b"")
        mock_audio = MagicMock()
        with patch("vripr.app.HAS_MUTAGEN", True), \
             patch("vripr.app.ID3", return_value=mock_audio):
            apply_tags(str(p), SAMPLE_TRACK)
        mock_audio.add.assert_called()
        mock_audio.save.assert_called_once()
