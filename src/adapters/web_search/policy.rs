use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

use crate::foundation::error::AppError;

pub(super) const MAX_QUERY_CHARS: usize = 400;
pub(super) const MAX_QUERY_WORDS: usize = 50;
const MAX_SOURCE_URL_BYTES: usize = 2_048;
const DEFAULT_HTTPS_PORT: u16 = 443;

pub(super) fn validate_query(query: &str) -> Result<&str, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::usage("웹 검색어가 필요합니다."));
    }
    if query.chars().count() > MAX_QUERY_CHARS || query.split_whitespace().count() > MAX_QUERY_WORDS
    {
        return Err(AppError::usage(format!(
            "웹 검색어는 최대 {MAX_QUERY_CHARS}자, {MAX_QUERY_WORDS}단어까지 허용합니다."
        )));
    }
    if query
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\n'))
    {
        return Err(AppError::usage(
            "웹 검색어에는 제어 문자를 사용할 수 없습니다.",
        ));
    }
    Ok(query)
}

pub(super) fn is_valid_https_source_url(url: &str) -> bool {
    validate_https_uri_shape(url)
        .and_then(|uri| validate_public_host(uri.authority().expect("validated authority").host()))
        .is_ok()
}

pub(super) fn validate_open_url(url: &str) -> Result<String, AppError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::usage("WebOpen URL이 필요합니다."));
    }
    let without_fragment = url.split('#').next().unwrap_or_default();
    let normalized = if without_fragment
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        format!("https://{}", &without_fragment[7..])
    } else {
        without_fragment.to_string()
    };
    let uri = validate_https_uri_shape(&normalized)?;
    validate_public_host(uri.authority().expect("validated authority").host())?;
    Ok(uri.to_string())
}

pub(super) fn validate_public_resolution(url: &str) -> Result<(), AppError> {
    let uri = validate_https_uri_shape(url)?;
    let authority = uri.authority().expect("validated authority");
    let host = authority.host();
    if parse_ip(host).is_some() {
        return Ok(());
    }
    let port = authority.port_u16().unwrap_or(DEFAULT_HTTPS_PORT);
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::runtime("WebOpen 대상 host의 DNS를 확인하지 못했습니다."))?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(AppError::runtime(
            "WebOpen 대상 host가 공개 IP로 해석되지 않았습니다.",
        ));
    }
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(AppError::blocked(
            "WebOpen은 내부·로컬 네트워크로 해석되는 host를 열지 않습니다.",
        ));
    }
    Ok(())
}

pub(super) fn resolve_redirect_url(current: &str, location: &str) -> Result<String, AppError> {
    let current_uri = validate_https_uri_shape(current)?;
    let location = location.trim();
    if location.is_empty() {
        return Err(AppError::blocked(
            "WebOpen redirect에 Location URL이 없습니다.",
        ));
    }
    let candidate = if location.starts_with("https://") || location.starts_with("http://") {
        location.to_string()
    } else if location.starts_with("//") {
        format!("https:{location}")
    } else {
        let authority = current_uri.authority().expect("validated authority");
        if location.starts_with('/') {
            format!("https://{authority}{location}")
        } else if location.starts_with('?') {
            format!("https://{authority}{}{}", current_uri.path(), location)
        } else {
            let base = current_uri.path();
            let directory = base.rsplit_once('/').map_or("/", |(directory, _)| {
                if directory.is_empty() {
                    "/"
                } else {
                    directory
                }
            });
            let path = if directory == "/" {
                format!("/{location}")
            } else {
                format!("{directory}/{location}")
            };
            format!("https://{authority}{path}")
        }
    };
    validate_open_url(&candidate)
}

pub(super) fn same_web_origin(left: &str, right: &str) -> bool {
    let Ok(left) = validate_https_uri_shape(left) else {
        return false;
    };
    let Ok(right) = validate_https_uri_shape(right) else {
        return false;
    };
    let Some(left_authority) = left.authority() else {
        return false;
    };
    let Some(right_authority) = right.authority() else {
        return false;
    };
    normalize_www(left_authority.host()) == normalize_www(right_authority.host())
        && left_authority.port_u16().unwrap_or(DEFAULT_HTTPS_PORT)
            == right_authority.port_u16().unwrap_or(DEFAULT_HTTPS_PORT)
}

fn validate_https_uri_shape(url: &str) -> Result<ureq::http::Uri, AppError> {
    if url.len() > MAX_SOURCE_URL_BYTES
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::usage(
            "WebOpen URL은 공백·제어 문자 없이 최대 2048 bytes까지 허용합니다.",
        ));
    }
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| AppError::usage("WebOpen URL 형식이 올바르지 않습니다."))?;
    if uri.scheme_str() != Some("https") {
        return Err(AppError::blocked("WebOpen은 HTTPS URL만 허용합니다."));
    }
    let authority = uri
        .authority()
        .filter(|authority| !authority.host().is_empty())
        .ok_or_else(|| AppError::usage("WebOpen URL에 host가 필요합니다."))?;
    if authority.as_str().contains('@') {
        return Err(AppError::blocked(
            "WebOpen URL에는 사용자 인증정보를 포함할 수 없습니다.",
        ));
    }
    Ok(uri)
}

fn validate_public_host(host: &str) -> Result<(), AppError> {
    if let Some(ip) = parse_ip(host) {
        return is_public_ip(ip)
            .then_some(())
            .ok_or_else(|| AppError::blocked("WebOpen은 내부·로컬 IP를 열지 않습니다."));
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if !host.contains('.')
        || host == "localhost"
        || [".localhost", ".local", ".internal", ".home", ".lan"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
        || host.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return Err(AppError::blocked(
            "WebOpen은 공개 DNS host만 열 수 있습니다.",
        ));
    }
    Ok(())
}

fn parse_ip(host: &str) -> Option<IpAddr> {
    host.trim_matches(['[', ']']).parse().ok()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, d] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224
        || (a == 255 && b == 255 && c == 255 && d == 255))
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn normalize_www(host: &str) -> String {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized
        .strip_prefix("www.")
        .unwrap_or(&normalized)
        .to_string()
}
