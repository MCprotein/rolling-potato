#[test]
fn browser_navigation_accepts_only_public_https_on_default_port() {
    assert_eq!(
        validate_browser_navigation_url("https://www.google.com/search?q=rust").unwrap(),
        "https://www.google.com/search?q=rust"
    );
    for blocked in [
        "http://www.google.com/",
        "https://localhost/",
        "https://127.0.0.1/",
        "https://www.google.com:8443/",
        "https://user:secret@www.google.com/",
    ] {
        assert!(
            validate_browser_navigation_url(blocked).is_err(),
            "{blocked}"
        );
    }
}

#[test]
fn rejects_empty_oversized_and_control_character_queries() {
    assert!(validate_query("").is_err());
    assert!(validate_query(&"가".repeat(MAX_QUERY_CHARS + 1)).is_err());
    assert!(validate_query(&vec!["word"; MAX_QUERY_WORDS + 1].join(" ")).is_err());
    assert!(validate_query("safe\u{0}unsafe").is_err());
    assert_eq!(validate_query(" Rust 검색 ").unwrap(), "Rust 검색");
}

#[test]
fn direct_search_is_available_without_api_credentials() {
    assert_eq!(
        configuration_summary(),
        "사용 가능; API key 없는 WebSearch·WebOpen·WebFind"
    );
}

#[test]
fn canonical_urls_have_stable_ids_and_drop_tracking_variants() {
    let tracked = "https://www.Example.com:443/docs/?b=2&utm_source=search&a=1#section";
    let canonical = "https://example.com/docs?a=1&b=2";

    assert_eq!(canonicalize_source_url(tracked).as_deref(), Some(canonical));
    assert_eq!(stable_source_id(tracked), stable_source_id(canonical));
    assert!(stable_source_id(canonical).starts_with("source-"));
    assert_eq!(stable_source_id(canonical).len(), "source-".len() + 16);
}

#[test]
fn direct_request_is_https_only_and_does_not_follow_redirects() {
    let config = direct_agent_config();

    assert!(config.https_only());
    assert_eq!(config.max_redirects(), 0);
}

#[test]
fn maps_status_without_exposing_provider_response() {
    for (status, expected) in [(429, "요청"), (400, "거부"), (500, "일시적")] {
        let message = map_search_error(ureq::Error::StatusCode(status)).message;
        assert!(message.contains(expected), "status={status}: {message}");
        assert!(!message.contains("secret"));
    }
}

#[test]
fn rejects_malformed_or_deceptive_https_sources() {
    for url in [
        "https://",
        "https://user@example.com/docs",
        "https://example.com/a path",
        "https://example.com/\nforged",
        "http://example.com/docs",
    ] {
        assert!(!is_valid_https_source_url(url), "url: {url}");
    }
    assert!(is_valid_https_source_url("https://example.com/docs?q=rust"));
}

#[test]
fn web_open_upgrades_http_and_rejects_private_or_credentialed_targets() {
    assert_eq!(
        validate_open_url("http://example.com/docs").unwrap(),
        "https://example.com/docs"
    );
    for url in [
        "https://user:secret@example.com/",
        "https://localhost/",
        "https://127.0.0.1/",
        "https://10.0.0.1/",
        "https://[::1]/",
        "file:///tmp/secret",
    ] {
        assert!(validate_open_url(url).is_err(), "url: {url}");
    }
}

#[test]
fn web_open_only_auto_follows_same_host_redirects() {
    let current = "https://docs.example.com/guide/start";
    let same = resolve_redirect_url(current, "/guide/next").unwrap();
    let www = resolve_redirect_url(current, "https://www.docs.example.com/guide").unwrap();
    let cross = resolve_redirect_url(current, "https://accounts.example.net/login").unwrap();

    assert!(same_web_origin(current, &same));
    assert!(same_web_origin(current, &www));
    assert!(!same_web_origin(current, &cross));
    assert_eq!(same, "https://docs.example.com/guide/next");
}

#[test]
fn web_open_transport_never_auto_follows_redirects() {
    let config = page_agent_config();

    assert!(config.https_only());
    assert_eq!(config.max_redirects(), 0);
    assert!(config.proxy().is_none());
}

#[test]
fn web_open_transport_rejects_any_private_dns_answer() {
    use std::net::SocketAddr;

    let public = "93.184.216.34:443".parse::<SocketAddr>().unwrap();
    let private = "127.0.0.1:443".parse::<SocketAddr>().unwrap();

    assert!(socket_addresses_are_public(&[public]));
    assert!(!socket_addresses_are_public(&[public, private]));
}
