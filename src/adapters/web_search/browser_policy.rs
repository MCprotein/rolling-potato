use std::net::{SocketAddr, ToSocketAddrs};

use crate::foundation::error::AppError;

use super::policy::{socket_addresses_are_public, validate_open_url, validate_public_host};

pub(crate) fn validate_browser_navigation_url(url: &str) -> Result<String, AppError> {
    let url = url.trim();
    if !url
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        return Err(AppError::blocked(
            "격리 브라우저 navigation은 public HTTPS URL만 허용합니다.",
        ));
    }
    let url = validate_open_url(url)?;
    let uri = url
        .parse::<ureq::http::Uri>()
        .map_err(|_| AppError::usage("격리 브라우저 URL 형식이 올바르지 않습니다."))?;
    if uri
        .authority()
        .and_then(|authority| authority.port_u16())
        .is_some_and(|port| port != 443)
    {
        return Err(AppError::blocked(
            "격리 브라우저 navigation은 HTTPS 기본 port 443만 허용합니다.",
        ));
    }
    Ok(url)
}

pub(crate) fn resolve_public_browser_target(
    host: &str,
    port: u16,
) -> Result<Vec<SocketAddr>, AppError> {
    if port != 443 {
        return Err(AppError::blocked(
            "격리 브라우저 proxy는 HTTPS 443 연결만 허용합니다.",
        ));
    }
    validate_public_host(host)?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::blocked("브라우저 대상 host를 공개 IP로 해석하지 못했습니다."))?
        .take(16)
        .collect::<Vec<_>>();
    if !socket_addresses_are_public(&addresses) {
        return Err(AppError::blocked(
            "브라우저 대상 host가 local 또는 private IP를 포함해 연결을 차단했습니다.",
        ));
    }
    Ok(addresses)
}
