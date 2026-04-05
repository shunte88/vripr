use vripr::workers::export::validate_path_template;

#[test]
fn test_valid_template_no_errors() {
    let errors = validate_path_template("{album_artist}/{album}/{tracknum} - {title}");
    assert!(errors.is_empty(), "All standard tokens should be valid");
}

#[test]
fn test_valid_template_with_brackets() {
    let errors = validate_path_template("{album_artist}/{album} [{country_iso}][{catalog}]/{tracknum} - {title}");
    assert!(errors.is_empty());
}

#[test]
fn test_empty_template_no_errors() {
    let errors = validate_path_template("");
    assert!(errors.is_empty());
}

#[test]
fn test_no_tokens_no_errors() {
    let errors = validate_path_template("static/path/filename");
    assert!(errors.is_empty());
}

#[test]
fn test_unknown_token_reported() {
    let errors = validate_path_template("{album_artist}/{bogus_field}/{title}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].token, "bogus_field");
}

#[test]
fn test_alias_track_number_suggests_tracknum() {
    let errors = validate_path_template("{track_number}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("tracknum"));
}

#[test]
fn test_alias_trackno_suggests_tracknum() {
    let errors = validate_path_template("{trackno}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("tracknum"));
}

#[test]
fn test_alias_catno_suggests_catalog() {
    let errors = validate_path_template("{catno}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("catalog"));
}

#[test]
fn test_alias_country_code_suggests_country_iso() {
    let errors = validate_path_template("{country_code}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("country_iso"));
}

#[test]
fn test_normalization_track_num_suggests_tracknum() {
    // track_num → strip underscore → tracknum → exact match
    let errors = validate_path_template("{track_num}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("tracknum"));
}

#[test]
fn test_prefix_cat_suggests_catalog() {
    let errors = validate_path_template("{cat}");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].suggestion.as_deref(), Some("catalog"));
}

#[test]
fn test_multiple_errors_reported() {
    let errors = validate_path_template("{track_number}/{bogus}/{album}");
    assert_eq!(errors.len(), 2, "should catch track_number and bogus");
    let tokens: Vec<&str> = errors.iter().map(|e| e.token.as_str()).collect();
    assert!(tokens.contains(&"track_number"));
    assert!(tokens.contains(&"bogus"));
}

#[test]
fn test_unclosed_brace_ignored() {
    // {title with no closing brace — not a complete token, skip
    let errors = validate_path_template("{album}/{title");
    assert!(errors.is_empty());
}

#[test]
fn test_all_supported_tokens_valid() {
    let template = vripr::workers::export::SUPPORTED_TOKENS
        .iter()
        .map(|t| format!("{{{}}}", t))
        .collect::<Vec<_>>()
        .join("/");
    let errors = validate_path_template(&template);
    assert!(errors.is_empty(), "Every supported token should pass validation");
}
