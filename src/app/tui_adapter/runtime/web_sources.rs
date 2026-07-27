use std::time::Duration;

use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiWebSourceOption;

pub(super) fn options(pages: &WebPageSession) -> Vec<TuiWebSourceOption> {
    pages
        .sources()
        .into_iter()
        .map(|source| TuiWebSourceOption {
            source_id: source.source_id,
            title: source.title,
            url: source.url,
            opened: source.opened,
            current: source.current,
        })
        .collect()
}

pub(super) fn select(pages: &mut WebPageSession, source_id: &str) -> Result<String, AppError> {
    let source = pages
        .source(source_id)
        .ok_or_else(|| AppError::usage("선택한 웹 출처가 현재 세션에 없습니다."))?;
    if !source.opened {
        return super::super::web_tools::execute(
            &mut WebResearchSession::default(),
            pages,
            WebToolRoute::Open { url: source.url },
            "선택한 웹 출처를 열고 요약해줘",
            "",
            Duration::ZERO,
        );
    }
    pages.select(source_id);
    let page = pages
        .current()
        .ok_or_else(|| AppError::blocked("현재 웹 출처를 읽지 못했습니다."))?;
    Ok(format!(
        "현재 웹 출처를 변경했습니다.\n- [{}] {}\n- {}\n/find <텍스트>로 이 문서 안을 찾을 수 있습니다.",
        page.source_id,
        page.title.as_deref().unwrap_or("제목 없음"),
        page.final_url
    ))
}
