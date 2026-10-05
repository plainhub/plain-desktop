use super::*;

#[test]
fn custom_bearer_requires_the_exact_second_header_token_and_a_custom_session() {
    assert!(valid_custom_bearer("Bearer secret", "secret"));
    assert!(!valid_custom_bearer("Bearer wrong", "secret"));
    assert!(!valid_custom_bearer("Bearer secret", ""));
    assert!(!valid_custom_bearer("Bearer", "secret"));
    assert!(!valid_custom_bearer("secret", "secret"));
    assert!(!valid_custom_bearer("Bearer  secret", "secret"));
}
