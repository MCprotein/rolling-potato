use crate::adapters::web_search::WebPageEvidence;
use crate::app::web_search_adapter::{self, WebToolRoute};
use crate::foundation::error::AppError;

pub(super) fn dispatch(
    opened_page: &mut Option<WebPageEvidence>,
    request: &str,
) -> Option<Result<String, AppError>> {
    let route = web_search_adapter::route_tool_request(request)?;
    Some(match route {
        WebToolRoute::Open { url } => web_search_adapter::open_page(&url, request).map(|answer| {
            if let Some(page) = answer.page {
                *opened_page = Some(page);
            }
            answer.report
        }),
        WebToolRoute::Find { query } => {
            web_search_adapter::find_in_page(opened_page.as_ref(), &query)
        }
    })
}
