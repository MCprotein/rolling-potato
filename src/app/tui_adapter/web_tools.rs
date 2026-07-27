use crate::app::web_search_adapter::{
    self, WebPageSession, WebResearchAdmission, WebResearchSession, WebToolRoute,
};
use crate::foundation::error::AppError;
use std::time::Duration;

pub(super) fn dispatch(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    request: &str,
    local_context: &str,
    conversation_context: &str,
    elapsed: Duration,
) -> Option<Result<String, AppError>> {
    let route = web_search_adapter::route_tool_request(request)?;
    Some(execute(
        research,
        pages,
        route,
        request,
        local_context,
        conversation_context,
        elapsed,
    ))
}

pub(super) fn execute(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    request: &str,
    local_context: &str,
    conversation_context: &str,
    elapsed: Duration,
) -> Result<String, AppError> {
    let current_document = pages.current_url();
    let route = match research.admit(route, current_document, elapsed) {
        WebResearchAdmission::Execute(route) => route,
        WebResearchAdmission::Stop(terminal) => return Err(terminal.into_error()),
    };
    let failed_route = route.clone();
    let result = match route {
        WebToolRoute::Search { query } => web_search_adapter::answer(
            web_search_adapter::WebAnswerInput::new(&query, request, local_context)
                .with_conversation_context(conversation_context),
            research,
            pages,
            elapsed,
        ),
        WebToolRoute::Open { url } => {
            web_search_adapter::open_page(&url, request, research).map(|answer| {
                if let Some(page) = answer.page {
                    research.record_opened_document(&page.final_url);
                    pages.record(page);
                }
                answer.report
            })
        }
        WebToolRoute::Find { query } => web_search_adapter::find_in_page(pages.current(), &query),
    };
    match result {
        Ok(answer) => {
            research.complete();
            Ok(answer)
        }
        Err(error) => {
            research.record_failed_input(&failed_route);
            Err(error)
        }
    }
}
