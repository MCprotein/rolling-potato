use std::time::{Duration, Instant};

use crate::adapters::web_search::validate_browser_navigation_url;
use crate::foundation::error::AppError;
use crate::foundation::serialization::{parse_value, Value};
use crate::runtime_core::browser::{
    AdmittedBrowserAction, BrowserAction, BrowserActionResult, BrowserInteractionSession,
    BrowserKey, ScrollDirection,
};

use super::discovery::discover_installed_browser;
use super::protocol::{CdpCommand, CdpMethod, CdpResponse, RestrictedCdpClient};
use super::session::{BrowserSession, BrowserSessionOptions};

mod accessibility;
mod protocol_values;

use accessibility::{extract_accessibility_text, interactive_targets};
use protocol_values::{box_center, json_string, object_string, object_string_value};

const PROTOCOL_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SCREENSHOT_BASE64_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct RestrictedBrowser {
    session: BrowserSession,
    driver: BrowserActionDriver<CdpActionTransport>,
    started_at: Instant,
}

impl RestrictedBrowser {
    pub(super) fn launch(options: BrowserSessionOptions) -> Result<Self, AppError> {
        let executable = discover_installed_browser()?;
        let session = BrowserSession::launch(&executable, options)?;
        let transport = CdpActionTransport::connect(&session, PROTOCOL_TIMEOUT)?;
        Ok(Self {
            session,
            driver: BrowserActionDriver::new(transport),
            started_at: Instant::now(),
        })
    }

    pub(super) fn execute(
        &mut self,
        action: BrowserAction,
    ) -> Result<BrowserActionResult, AppError> {
        let _keep_session_alive = &self.session;
        self.driver.execute(action, self.started_at.elapsed())
    }
}

trait BrowserActionPort {
    fn call(&mut self, method: CdpMethod, params_json: String) -> Result<Value, AppError>;
    fn close_target(&mut self) -> Result<(), AppError>;
}

struct CdpActionTransport {
    client: RestrictedCdpClient,
    target_id: String,
    session_id: String,
    next_id: u64,
}

impl CdpActionTransport {
    fn connect(session: &BrowserSession, timeout: Duration) -> Result<Self, AppError> {
        let mut client = RestrictedCdpClient::connect(session.endpoint(), timeout)?;
        let target = send_command(
            &mut client,
            1,
            CdpMethod::TargetCreateTarget,
            "{\"url\":\"about:blank\"}".to_string(),
            None,
        )?;
        let target_id = object_string(&target, "targetId", "Target.createTarget")?;
        let attached = send_command(
            &mut client,
            2,
            CdpMethod::TargetAttachToTarget,
            format!(
                "{{\"targetId\":{},\"flatten\":true}}",
                json_string(&target_id)
            ),
            None,
        )?;
        let session_id = object_string(&attached, "sessionId", "Target.attachToTarget")?;
        let mut transport = Self {
            client,
            target_id,
            session_id,
            next_id: 3,
        };
        for method in [
            CdpMethod::PageEnable,
            CdpMethod::DomEnable,
            CdpMethod::AccessibilityEnable,
        ] {
            transport.call(method, "{}".to_string())?;
        }
        Ok(transport)
    }
}

impl BrowserActionPort for CdpActionTransport {
    fn call(&mut self, method: CdpMethod, params_json: String) -> Result<Value, AppError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        send_command(
            &mut self.client,
            id,
            method,
            params_json,
            Some(&self.session_id),
        )
    }

    fn close_target(&mut self) -> Result<(), AppError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        send_command(
            &mut self.client,
            id,
            CdpMethod::TargetCloseTarget,
            format!("{{\"targetId\":{}}}", json_string(&self.target_id)),
            None,
        )
        .map(|_| ())
    }
}

fn send_command(
    client: &mut RestrictedCdpClient,
    id: u64,
    method: CdpMethod,
    params_json: String,
    session_id: Option<&str>,
) -> Result<Value, AppError> {
    let command = CdpCommand::new(id, method, params_json)?;
    let command = if let Some(session_id) = session_id {
        command.with_session_id(session_id)?
    } else {
        command
    };
    response_result(client.send_command(&command)?)
}

fn response_result(response: CdpResponse) -> Result<Value, AppError> {
    let Value::Object(envelope) = parse_value(&response.raw_json, "CDP response")? else {
        return Err(AppError::runtime("CDP response root가 object가 아닙니다."));
    };
    if envelope.get("error").is_some() {
        return Err(AppError::runtime(
            "격리 브라우저가 허용된 CDP action을 거부했습니다.",
        ));
    }
    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| AppError::runtime("CDP response에 result가 없습니다."))
}

struct BrowserActionDriver<T> {
    transport: T,
    interaction: BrowserInteractionSession,
}

impl<T: BrowserActionPort> BrowserActionDriver<T> {
    fn new(transport: T) -> Self {
        Self {
            transport,
            interaction: BrowserInteractionSession::default(),
        }
    }

    fn execute(
        &mut self,
        action: BrowserAction,
        elapsed: Duration,
    ) -> Result<BrowserActionResult, AppError> {
        let action = normalize_navigation(action)?;
        let admitted = self
            .interaction
            .admit(action, elapsed)
            .map_err(|blocked| AppError::blocked(blocked.message()))?;
        match admitted {
            AdmittedBrowserAction::Navigate { url } => {
                self.transport.call(
                    CdpMethod::PageNavigate,
                    format!("{{\"url\":{}}}", json_string(&url)),
                )?;
                self.interaction.invalidate_handles();
                Ok(BrowserActionResult::Navigated { url })
            }
            AdmittedBrowserAction::Observe => {
                let tree = self
                    .transport
                    .call(CdpMethod::AccessibilityGetFullAxTree, "{}".to_string())?;
                let observation = self
                    .interaction
                    .install_observation(interactive_targets(&tree)?);
                Ok(BrowserActionResult::Observation(observation))
            }
            AdmittedBrowserAction::Click { target } => {
                let model = self.transport.call(
                    CdpMethod::DomGetBoxModel,
                    format!("{{\"backendNodeId\":{}}}", target.target_ref),
                )?;
                let (x, y) = box_center(&model)?;
                for event in ["mousePressed", "mouseReleased"] {
                    self.transport.call(
                        CdpMethod::InputDispatchMouseEvent,
                        format!(
                            "{{\"type\":\"{event}\",\"x\":{x},\"y\":{y},\"button\":\"left\",\"clickCount\":1}}"
                        ),
                    )?;
                }
                self.interaction.invalidate_handles();
                Ok(BrowserActionResult::Clicked)
            }
            AdmittedBrowserAction::Type { target, text } => {
                self.transport.call(
                    CdpMethod::DomFocus,
                    format!("{{\"backendNodeId\":{}}}", target.target_ref),
                )?;
                self.transport.call(
                    CdpMethod::InputInsertText,
                    format!("{{\"text\":{}}}", json_string(&text)),
                )?;
                self.interaction.invalidate_handles();
                Ok(BrowserActionResult::Typed)
            }
            AdmittedBrowserAction::Press { key } => {
                let (key, code, virtual_key) = key_contract(key);
                for event in ["keyDown", "keyUp"] {
                    self.transport.call(
                        CdpMethod::InputDispatchKeyEvent,
                        format!(
                            "{{\"type\":\"{event}\",\"key\":\"{key}\",\"code\":\"{code}\",\"windowsVirtualKeyCode\":{virtual_key},\"nativeVirtualKeyCode\":{virtual_key}}}"
                        ),
                    )?;
                }
                self.interaction.invalidate_handles();
                Ok(BrowserActionResult::KeyPressed)
            }
            AdmittedBrowserAction::Scroll {
                direction,
                viewports,
            } => {
                let sign = match direction {
                    ScrollDirection::Up => -1_i32,
                    ScrollDirection::Down => 1_i32,
                };
                let delta = sign * i32::from(viewports) * 600;
                self.transport.call(
                    CdpMethod::InputDispatchMouseEvent,
                    format!(
                        "{{\"type\":\"mouseWheel\",\"x\":0,\"y\":0,\"deltaX\":0,\"deltaY\":{delta}}}"
                    ),
                )?;
                self.interaction.invalidate_handles();
                Ok(BrowserActionResult::Scrolled)
            }
            AdmittedBrowserAction::Extract { max_chars } => {
                let tree = self
                    .transport
                    .call(CdpMethod::AccessibilityGetFullAxTree, "{}".to_string())?;
                Ok(BrowserActionResult::Extracted {
                    text: extract_accessibility_text(&tree, max_chars)?,
                })
            }
            AdmittedBrowserAction::Screenshot => {
                let result = self.transport.call(
                    CdpMethod::PageCaptureScreenshot,
                    "{\"format\":\"png\",\"fromSurface\":true,\"captureBeyondViewport\":false}"
                        .to_string(),
                )?;
                let png_base64 = object_string_value(&result, "data", "Page.captureScreenshot")?;
                if png_base64.len() > MAX_SCREENSHOT_BASE64_BYTES {
                    return Err(AppError::blocked(
                        "브라우저 screenshot이 허용 크기를 초과했습니다.",
                    ));
                }
                Ok(BrowserActionResult::Screenshot { png_base64 })
            }
            AdmittedBrowserAction::Close => {
                self.transport.close_target()?;
                Ok(BrowserActionResult::Closed)
            }
        }
    }
}

fn normalize_navigation(action: BrowserAction) -> Result<BrowserAction, AppError> {
    match action {
        BrowserAction::Navigate { url } => {
            validate_browser_navigation_url(&url).map(|url| BrowserAction::Navigate { url })
        }
        other => Ok(other),
    }
}

fn key_contract(key: BrowserKey) -> (&'static str, &'static str, u16) {
    match key {
        BrowserKey::Enter => ("Enter", "Enter", 13),
        BrowserKey::Escape => ("Escape", "Escape", 27),
        BrowserKey::Tab => ("Tab", "Tab", 9),
        BrowserKey::Backspace => ("Backspace", "Backspace", 8),
        BrowserKey::ArrowUp => ("ArrowUp", "ArrowUp", 38),
        BrowserKey::ArrowDown => ("ArrowDown", "ArrowDown", 40),
        BrowserKey::ArrowLeft => ("ArrowLeft", "ArrowLeft", 37),
        BrowserKey::ArrowRight => ("ArrowRight", "ArrowRight", 39),
    }
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;
