use crate::adapters::web_search::WebPageEvidence;
use crate::app::web_search_adapter::{
    self, WebResearchAdmission, WebResearchSession, WebToolRoute,
};
use crate::foundation::error::AppError;
use std::time::Duration;

pub(super) fn dispatch(
    research: &mut WebResearchSession,
    opened_page: &mut Option<WebPageEvidence>,
    request: &str,
    local_context: &str,
    elapsed: Duration,
) -> Option<Result<String, AppError>> {
    let route = web_search_adapter::route_tool_request(request)?;
    Some(execute(
        research,
        opened_page,
        route,
        request,
        local_context,
        elapsed,
    ))
}

pub(super) fn execute(
    research: &mut WebResearchSession,
    opened_page: &mut Option<WebPageEvidence>,
    route: WebToolRoute,
    request: &str,
    local_context: &str,
    elapsed: Duration,
) -> Result<String, AppError> {
    let current_document = opened_page.as_ref().map(|page| page.final_url.as_str());
    let route = match research.admit(route, current_document, elapsed) {
        WebResearchAdmission::Execute(route) => route,
        WebResearchAdmission::Stop(terminal) => return Err(terminal.into_error()),
    };
    let failed_route = route.clone();
    let result = match route {
        WebToolRoute::Search { query } => web_search_adapter::answer(
            web_search_adapter::WebAnswerInput::new(&query, request, local_context),
            research,
        ),
        WebToolRoute::Open { url } => {
            web_search_adapter::open_page(&url, request, research).map(|answer| {
                if let Some(page) = answer.page {
                    research.record_opened_document(&page.final_url);
                    *opened_page = Some(page);
                }
                answer.report
            })
        }
        WebToolRoute::Find { query } => {
            web_search_adapter::find_in_page(opened_page.as_ref(), &query)
        }
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
