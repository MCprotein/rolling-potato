use crate::app::web_search_adapter::{
    self, WebGroundingEvidence, WebPageSession, WebResearchAdmission, WebResearchSession,
    WebToolRoute,
};
use crate::foundation::error::AppError;
use std::time::Duration;

pub(super) struct WebToolExecution {
    pub(super) response: String,
    pub(super) grounding: Vec<WebGroundingEvidence>,
}

pub(super) fn execute(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    request: &str,
    local_context: &str,
    conversation_context: &str,
    elapsed: Duration,
) -> Result<WebToolExecution, AppError> {
    let route = web_search_adapter::validate_public_web_step(route)?;
    let current_document = pages.current_url();
    let route = match research.admit(route, current_document, elapsed) {
        WebResearchAdmission::Execute(route) => route,
        WebResearchAdmission::Stop(terminal) => return Err(terminal.into_error()),
    };
    let failed_route = route.clone();
    let result = match route {
        WebToolRoute::Search { query } => {
            let answer = web_search_adapter::answer(
                web_search_adapter::WebAnswerInput::new(&query, request, local_context)
                    .with_conversation_context(conversation_context),
                research,
                pages,
                elapsed,
            )?;
            Ok(WebToolExecution {
                response: answer.response,
                grounding: answer.grounding,
            })
        }
        WebToolRoute::Open { url } => {
            web_search_adapter::open_page(&url, request, research).map(|answer| {
                let grounding = answer
                    .page
                    .as_ref()
                    .map(|page| WebGroundingEvidence {
                        source_id: page.source_id.clone(),
                        title: page
                            .title
                            .clone()
                            .unwrap_or_else(|| "제목 없음".to_string()),
                        url: page.final_url.clone(),
                        excerpt: page.content.chars().take(1_536).collect(),
                    })
                    .into_iter()
                    .collect();
                if let Some(page) = answer.page {
                    research.record_opened_document(&page.final_url);
                    pages.record(page);
                }
                WebToolExecution {
                    response: answer.report,
                    grounding,
                }
            })
        }
        WebToolRoute::Find { query } => {
            let grounding = pages
                .current()
                .map(|page| WebGroundingEvidence {
                    source_id: page.source_id.clone(),
                    title: page
                        .title
                        .clone()
                        .unwrap_or_else(|| "제목 없음".to_string()),
                    url: page.final_url.clone(),
                    excerpt: page.content.chars().take(1_536).collect(),
                })
                .into_iter()
                .collect();
            web_search_adapter::answer_find_in_page(pages.current(), &query, request).map(
                |response| WebToolExecution {
                    response,
                    grounding,
                },
            )
        }
    };
    match result {
        Ok(execution) => {
            research.complete();
            Ok(execution)
        }
        Err(error) => {
            research.record_failed_input(&failed_route);
            Err(error)
        }
    }
}
