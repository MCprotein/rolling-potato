use crate::app::web_search_adapter::{
    self, WebGroundingEvidence, WebPageSession, WebResearchAdmission, WebResearchSession,
    WebToolObservation, WebToolRoute,
};
use crate::foundation::error::AppError;
use std::time::Duration;

pub(super) struct WebToolExecution {
    pub(super) response: String,
    pub(super) grounding: Vec<WebGroundingEvidence>,
}

pub(super) fn observe(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    request: &str,
    local_context: &str,
    conversation_context: &str,
    elapsed: Duration,
) -> Result<WebToolObservation, AppError> {
    let route = web_search_adapter::validate_public_web_step(route)?;
    let current_document = pages.current_url();
    let route = match research.admit(route, current_document, elapsed) {
        WebResearchAdmission::Execute(route) => route,
        WebResearchAdmission::Stop(terminal) => return Err(terminal.into_error()),
    };
    let failed_route = route.clone();
    let result = match route {
        WebToolRoute::Search { query } => web_search_adapter::observe_search(
            web_search_adapter::WebAnswerInput::new(&query, request, local_context)
                .with_conversation_context(conversation_context),
            research,
            pages,
            elapsed,
        )
        .map(WebToolObservation::Evidence),
        WebToolRoute::Open { url } => {
            web_search_adapter::observe_open_page(&url, request, research).map(|observed| {
                if let Some(page) = observed.page {
                    research.record_opened_document(&page.final_url);
                    pages.record(page);
                }
                observed.observation
            })
        }
        WebToolRoute::Find { query } => {
            web_search_adapter::observe_find_in_page(pages.current(), &query, request)
        }
    };
    match result {
        Ok(observation) => {
            research.complete();
            Ok(observation)
        }
        Err(error) => {
            research.record_failed_input(&failed_route);
            Err(error)
        }
    }
}

pub(super) fn answer(observation: WebToolObservation, request: &str) -> WebToolExecution {
    let answer = web_search_adapter::answer_observation(observation, request);
    WebToolExecution {
        response: answer.response,
        grounding: answer.grounding,
    }
}
