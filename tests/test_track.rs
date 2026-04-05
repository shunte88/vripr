use vripr::track::TrackMeta;

#[test]
fn test_duration() {
    let t = TrackMeta {
        index: 1,
        start: 1.5,
        end: 4.5,
        ..Default::default()
    };
    let diff = (t.duration() - 3.0).abs();
    assert!(diff < 1e-10, "Expected duration 3.0, got {}", t.duration());
}

#[test]
fn test_display_time() {
    let t = TrackMeta {
        index: 1,
        start: 65.0,
        end: 185.0,
        ..Default::default()
    };
    assert_eq!(t.display_time(), "1:05–3:05");
}

#[test]
fn test_display_time_start_zero() {
    let t = TrackMeta {
        index: 1,
        start: 0.0,
        end: 125.0,
        ..Default::default()
    };
    assert_eq!(t.display_time(), "0:00–2:05");
}

#[test]
fn test_status_icon_default() {
    let t = TrackMeta {
        index: 1,
        start: 0.0,
        end: 60.0,
        ..Default::default()
    };
    assert_eq!(t.status_icon(), "");
}

#[test]
fn test_status_icon_fingerprinted() {
    let t = TrackMeta {
        index: 1,
        start: 0.0,
        end: 60.0,
        fingerprint_done: true,
        ..Default::default()
    };
    assert_eq!(t.status_icon(), "🔍");
}

#[test]
fn test_status_icon_exported() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let t = TrackMeta {
        index: 1,
        start: 0.0,
        end: 60.0,
        export_path: Some(tmp.path().to_path_buf()),
        ..Default::default()
    };
    assert_eq!(t.status_icon(), "✓");
}

#[test]
fn test_default_fields() {
    let t = TrackMeta {
        index: 3,
        start: 0.0,
        end: 1.0,
        ..Default::default()
    };
    assert_eq!(t.title, "");
    assert_eq!(t.artist, "");
    assert_eq!(t.album, "");
    assert_eq!(t.album_artist, "");
    assert_eq!(t.genre, "");
    assert_eq!(t.track_number, "");
    assert_eq!(t.year, "");
    assert!(!t.fingerprint_done);
    assert!(t.export_path.is_none());
}

#[test]
fn test_track_index() {
    let t = TrackMeta {
        index: 7,
        start: 0.0,
        end: 1.0,
        ..Default::default()
    };
    assert_eq!(t.index, 7);
}

#[test]
fn test_duration_large() {
    // A typical vinyl side: ~25 minutes
    let t = TrackMeta {
        index: 1,
        start: 10.0,
        end: 250.0,
        ..Default::default()
    };
    let diff = (t.duration() - 240.0).abs();
    assert!(diff < 1e-10, "Expected 240.0, got {}", t.duration());
}
