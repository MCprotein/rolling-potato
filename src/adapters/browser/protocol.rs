use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::foundation::error::AppError;
use crate::foundation::serialization::{parse_value, Value};

use super::websocket::LocalWebSocket;

const MAX_COMMAND_BYTES: usize = 32 * 1024;
const MAX_UNMATCHED_MESSAGES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CdpEndpoint {
    port: u16,
    path: String,
}

impl CdpEndpoint {
    pub(super) fn from_active_port_file(contents: &str) -> Result<Self, AppError> {
        let mut lines = contents.lines();
        let port = lines
            .next()
            .map(str::trim)
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .ok_or_else(|| {
                AppError::blocked("DevToolsActivePort의 loopback port가 올바르지 않습니다.")
            })?;
        let path = lines.next().map(str::trim).ok_or_else(|| {
            AppError::blocked("DevToolsActivePort에 browser endpoint가 없습니다.")
        })?;
        if !valid_browser_path(path) {
            return Err(AppError::blocked(
                "DevToolsActivePort의 browser endpoint가 허용된 형식이 아닙니다.",
            ));
        }
        if lines.any(|line| !line.trim().is_empty()) {
            return Err(AppError::blocked(
                "DevToolsActivePort에 예상하지 않은 추가 필드가 있습니다.",
            ));
        }
        Ok(Self {
            port,
            path: path.to_string(),
        })
    }

    fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.port)
    }

    fn path(&self) -> &str {
        &self.path
    }

    #[cfg(test)]
    pub(super) fn display_url(&self) -> String {
        format!("ws://127.0.0.1:{}{}", self.port, self.path)
    }
}

fn valid_browser_path(path: &str) -> bool {
    path.strip_prefix("/devtools/browser/").is_some_and(|id| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CdpMethod {
    TargetCreateTarget,
    TargetCloseTarget,
    TargetAttachToTarget,
    PageEnable,
    PageNavigate,
    PageGetNavigationHistory,
    PageCaptureScreenshot,
    AccessibilityEnable,
    AccessibilityGetFullAxTree,
    DomEnable,
    DomGetBoxModel,
    DomFocus,
    InputInsertText,
    InputDispatchKeyEvent,
    InputDispatchMouseEvent,
}

impl CdpMethod {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TargetCreateTarget => "Target.createTarget",
            Self::TargetCloseTarget => "Target.closeTarget",
            Self::TargetAttachToTarget => "Target.attachToTarget",
            Self::PageEnable => "Page.enable",
            Self::PageNavigate => "Page.navigate",
            Self::PageGetNavigationHistory => "Page.getNavigationHistory",
            Self::PageCaptureScreenshot => "Page.captureScreenshot",
            Self::AccessibilityEnable => "Accessibility.enable",
            Self::AccessibilityGetFullAxTree => "Accessibility.getFullAXTree",
            Self::DomEnable => "DOM.enable",
            Self::DomGetBoxModel => "DOM.getBoxModel",
            Self::DomFocus => "DOM.focus",
            Self::InputInsertText => "Input.insertText",
            Self::InputDispatchKeyEvent => "Input.dispatchKeyEvent",
            Self::InputDispatchMouseEvent => "Input.dispatchMouseEvent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CdpCommand {
    id: u64,
    method: CdpMethod,
    params_json: String,
    session_id: Option<String>,
}

impl CdpCommand {
    pub(super) fn new(
        id: u64,
        method: CdpMethod,
        params_json: impl Into<String>,
    ) -> Result<Self, AppError> {
        if id == 0 {
            return Err(AppError::usage("CDP command id는 0일 수 없습니다."));
        }
        let params_json = params_json.into();
        if params_json.len() > MAX_COMMAND_BYTES {
            return Err(AppError::blocked(
                "CDP command parameter가 허용 크기를 초과했습니다.",
            ));
        }
        if !matches!(
            parse_value(&params_json, "CDP command parameter")?,
            Value::Object(_)
        ) {
            return Err(AppError::blocked(
                "CDP command parameter는 JSON object여야 합니다.",
            ));
        }
        Ok(Self {
            id,
            method,
            params_json,
            session_id: None,
        })
    }

    pub(super) fn with_session_id(mut self, session_id: &str) -> Result<Self, AppError> {
        if session_id.is_empty()
            || session_id.len() > 128
            || !session_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(AppError::blocked(
                "CDP session identifier가 허용된 형식이 아닙니다.",
            ));
        }
        self.session_id = Some(session_id.to_string());
        Ok(self)
    }

    fn render(&self) -> Result<String, AppError> {
        let session = self
            .session_id
            .as_deref()
            .map(|session_id| format!(",\"sessionId\":\"{session_id}\""))
            .unwrap_or_default();
        let rendered = format!(
            "{{\"id\":{},\"method\":\"{}\",\"params\":{}{session}}}",
            self.id,
            self.method.as_str(),
            self.params_json,
        );
        if rendered.len() > MAX_COMMAND_BYTES {
            return Err(AppError::blocked("CDP command가 허용 크기를 초과했습니다."));
        }
        Ok(rendered)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CdpResponse {
    pub(super) raw_json: String,
}

pub(super) struct RestrictedCdpClient {
    websocket: LocalWebSocket,
}

impl RestrictedCdpClient {
    pub(super) fn connect(endpoint: &CdpEndpoint, timeout: Duration) -> Result<Self, AppError> {
        if timeout.is_zero() {
            return Err(AppError::usage("CDP protocol timeout은 0보다 커야 합니다."));
        }
        LocalWebSocket::connect(endpoint.socket_addr(), endpoint.path(), timeout)
            .map(|websocket| Self { websocket })
    }

    pub(super) fn send_command(&mut self, command: &CdpCommand) -> Result<CdpResponse, AppError> {
        self.websocket.send_text(&command.render()?)?;
        for _ in 0..MAX_UNMATCHED_MESSAGES {
            let message = self.websocket.read_text()?;
            if response_id(&message) == Some(command.id) {
                return Ok(CdpResponse { raw_json: message });
            }
        }
        Err(AppError::runtime(
            "CDP response가 제한된 message 수 안에 도착하지 않았습니다.",
        ))
    }
}

fn response_id(message: &str) -> Option<u64> {
    let Value::Object(object) = parse_value(message, "CDP response").ok()? else {
        return None;
    };
    let Value::Number(id) = object.get("id")? else {
        return None;
    };
    u64::try_from(*id).ok()
}

#[cfg(test)]
pub(super) fn test_endpoint(contents: &str) -> Result<CdpEndpoint, AppError> {
    CdpEndpoint::from_active_port_file(contents)
}
