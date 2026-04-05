pub mod build_info {
    include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
}

pub mod audio;
pub mod config;
pub mod metadata;
pub mod pipe;
pub mod tagging;
pub mod track;
pub mod workers;
// Note: `app` and `ui` are excluded from lib because they depend on egui
// which requires a display context. Integration tests for those are done
// through the binary.
