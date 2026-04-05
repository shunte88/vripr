use vripr::pipe::AudacityPipe;

#[test]
fn test_pipe_paths_linux() {
    let (to_path, from_path) = AudacityPipe::pipe_paths();

    let to_str = to_path.to_string_lossy();
    let from_str = from_path.to_string_lossy();

    // On Linux both paths should contain "audacity_script_pipe"
    #[cfg(target_os = "linux")]
    {
        assert!(
            to_str.contains("audacity_script_pipe"),
            "to_path should contain 'audacity_script_pipe', got: {}",
            to_str
        );
        assert!(
            from_str.contains("audacity_script_pipe"),
            "from_path should contain 'audacity_script_pipe', got: {}",
            from_str
        );

        // Should contain the UID
        let uid = nix::unistd::getuid().as_raw();
        assert!(
            to_str.contains(&uid.to_string()),
            "to_path should contain uid {}, got: {}",
            uid,
            to_str
        );
        assert!(
            from_str.contains(&uid.to_string()),
            "from_path should contain uid {}, got: {}",
            uid,
            from_str
        );
    }

    // Paths should not be equal
    assert_ne!(to_path, from_path, "to and from pipe paths should differ");
}

#[test]
fn test_check_pipes_no_audacity() {
    // When Audacity is not running, pipes should not exist
    // (unless someone is running tests with Audacity active, but that's rare)
    // We just verify the function returns without panicking.
    let result = AudacityPipe::check_pipes();
    // result is either true or false; we can't assert which without Audacity
    // But we can assert the function doesn't panic
    let _ = result;
}

#[test]
fn test_new_pipe_disconnected() {
    let pipe = AudacityPipe::new();
    assert!(
        !pipe.is_connected(),
        "Freshly created AudacityPipe should not be connected"
    );
}

#[test]
fn test_connect_fails_without_audacity() {
    let mut pipe = AudacityPipe::new();
    // This will fail because Audacity isn't running in test environment
    // We just verify it returns an error, not panics
    if !AudacityPipe::check_pipes() {
        let result = pipe.connect();
        assert!(result.is_err(), "connect() should fail when Audacity isn't running");
        assert!(!pipe.is_connected());
    }
}

#[test]
fn test_disconnect_when_not_connected() {
    let mut pipe = AudacityPipe::new();
    // Should not panic when disconnecting an already-disconnected pipe
    pipe.disconnect();
    assert!(!pipe.is_connected());
}

#[test]
fn test_pipe_paths_are_absolute() {
    let (to_path, from_path) = AudacityPipe::pipe_paths();
    assert!(to_path.is_absolute(), "to_path should be absolute: {:?}", to_path);
    assert!(from_path.is_absolute(), "from_path should be absolute: {:?}", from_path);
}
