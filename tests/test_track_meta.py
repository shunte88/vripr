"""Unit tests for vripr.app — non-GUI logic."""
import sys
import pytest

# Guard: skip entire module if PyQt6 not installed
pytest.importorskip("PyQt6")

from vripr.app import TrackMeta, TrackTableModel


# ── TrackMeta ──────────────────────────────────────────────────────────────

class TestTrackMeta:
    def test_duration(self):
        t = TrackMeta(index=1, start=10.0, end=250.0)
        assert t.duration == pytest.approx(240.0)

    def test_display_time_format(self):
        t = TrackMeta(index=1, start=65.0, end=185.0)
        assert t.display_time == "1:05–3:05"

    def test_status_icon_default(self):
        t = TrackMeta(index=1, start=0.0, end=60.0)
        assert t.status_icon == ""

    def test_status_icon_fingerprinted(self):
        t = TrackMeta(index=1, start=0.0, end=60.0, fingerprint_done=True)
        assert t.status_icon == "🔍"

    def test_status_icon_exported(self, tmp_path):
        p = tmp_path / "track.flac"
        p.write_bytes(b"")
        t = TrackMeta(index=1, start=0.0, end=60.0, export_path=str(p))
        assert t.status_icon == "✓"

    def test_default_fields(self):
        t = TrackMeta(index=3, start=0.0, end=1.0)
        assert t.title        == ""
        assert t.artist       == ""
        assert t.album        == ""
        assert t.album_artist == ""
        assert t.genre        == ""
        assert t.track_number == ""
        assert t.year         == ""


# ── TrackTableModel ────────────────────────────────────────────────────────

class TestTrackTableModel:
    @pytest.fixture()
    def model(self):
        tracks = [
            TrackMeta(index=1, start=0.0,   end=180.0, title="Song A",
                      artist="Artist X", track_number="1"),
            TrackMeta(index=2, start=185.0, end=360.0, title="Song B",
                      artist="Artist X", track_number="2"),
        ]
        return TrackTableModel(tracks), tracks

    def test_row_count(self, model):
        m, tracks = model
        assert m.rowCount() == 2

    def test_column_count(self, model):
        m, _ = model
        assert m.columnCount() == 8   # _COLS has 8 entries

    def test_data_title(self, model):
        from PyQt6.QtCore import Qt, QModelIndex
        m, tracks = model
        idx = m.index(0, 3)           # _TITLE_COL = 3
        assert m.data(idx, Qt.ItemDataRole.DisplayRole) == "Song A"

    def test_set_data(self, model):
        from PyQt6.QtCore import Qt
        m, tracks = model
        idx = m.index(1, 3)           # title col, row 1
        m.setData(idx, "New Title", Qt.ItemDataRole.EditRole)
        assert tracks[1].title == "New Title"

    def test_insert_track(self, model):
        m, tracks = model
        t = TrackMeta(index=3, start=365.0, end=500.0, title="Song C")
        m.insert_track(t)
        assert m.rowCount() == 3
        assert tracks[-1].title == "Song C"

    def test_remove_row(self, model):
        m, tracks = model
        m.remove_row(0)
        assert m.rowCount() == 1
        assert tracks[0].title == "Song B"

    def test_move_row_down(self, model):
        m, tracks = model
        new_row = m.move_row(0, 1)
        assert new_row == 1
        assert tracks[0].title == "Song B"
        assert tracks[1].title == "Song A"

    def test_move_row_up_boundary(self, model):
        m, tracks = model
        # moving row 0 up should be a no-op
        new_row = m.move_row(0, -1)
        assert new_row == 0
        assert tracks[0].title == "Song A"


# ── config ─────────────────────────────────────────────────────────────────

class TestConfig:
    def test_load_defaults(self, tmp_path, monkeypatch):
        monkeypatch.setattr("vripr.app.CONFIG_PATH", tmp_path / "vripr.ini")
        from vripr.app import load_config, DEFAULTS
        cfg = load_config()
        for k, v in DEFAULTS.items():
            assert cfg["vinyl_ripper"].get(k, v) == v

    def test_save_and_reload(self, tmp_path, monkeypatch):
        ini = tmp_path / "vripr.ini"
        monkeypatch.setattr("vripr.app.CONFIG_PATH", ini)
        from vripr.app import load_config, save_config
        cfg = load_config()
        cfg["vinyl_ripper"]["default_artist"] = "Test Artist"
        save_config(cfg)
        cfg2 = load_config()
        assert cfg2["vinyl_ripper"]["default_artist"] == "Test Artist"
