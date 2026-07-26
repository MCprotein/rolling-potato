//! Installed Chromium discovery and isolated DevTools session ownership.

mod actions;
mod discovery;
mod protocol;
mod proxy;
mod session;
mod websocket;

pub(crate) use actions::RestrictedBrowser;
pub(crate) use session::BrowserSessionOptions;

#[cfg(test)]
mod tests;
