use crate::app::web_search_adapter::{
    self, WebGroundingEvidence, WebPageSession, WebResearchAdmission, WebResearchSession,
    WebResearchTraceStatus, WebResearchTraceStep, WebToolObservation, WebToolRoute,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::surfaces::tui::runtime_bridge::{TuiRequestProgress, TuiRequestProgressReporter};
use std::time::Duration;

pub(super) struct WebToolExecution {
    pub(super) response: String,
    pub(super) grounding: Vec<WebGroundingEvidence>,
}

pub(super) struct WebTurnContext<'a> {
    pub(super) request: &'a str,
    pub(super) local_context: &'a str,
    pub(super) conversation_context: &'a str,
    pub(super) elapsed: Duration,
    pub(super) progress: &'a TuiRequestProgressReporter,
    pub(super) cancellation: &'a RequestCancellationToken,
}

pub(super) fn observe(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    context: WebTurnContext<'_>,
    trace: &mut Vec<WebResearchTraceStep>,
) -> Result<WebToolObservation, AppError> {
    context.cancellation.check()?;
    let route = web_search_adapter::validate_public_web_step(route)?;
    let current_document = pages.current_url();
    let route = match research.admit(route, current_document, context.elapsed) {
        WebResearchAdmission::Execute(route) => route,
        WebResearchAdmission::Stop(terminal) => return Err(terminal.into_error()),
    };
    let failed_route = route.clone();
    let result = match route {
        WebToolRoute::Search { query } => {
            let mut report_web_phase = |phase| {
                context.progress.emit(match phase {
                    web_search_adapter::WebResearchPhase::Searching => {
                        TuiRequestProgress::Searching
                    }
                    web_search_adapter::WebResearchPhase::Opening => TuiRequestProgress::Opening,
                    web_search_adapter::WebResearchPhase::Finding => TuiRequestProgress::Finding,
                });
            };
            web_search_adapter::observe_search(
                web_search_adapter::WebAnswerInput::new(
                    &query,
                    context.request,
                    context.local_context,
                )
                .with_conversation_context(context.conversation_context),
                research,
                pages,
                context.elapsed,
                &mut report_web_phase,
                trace,
                context.cancellation,
            )
            .map(WebToolObservation::Evidence)
        }
        WebToolRoute::Open { url } => {
            context.progress.emit(TuiRequestProgress::Opening);
            let route = WebToolRoute::Open { url: url.clone() };
            web_search_adapter::observe_open_page(&url, context.request, research).map(|observed| {
                let source_ids = observed
                    .page
                    .as_ref()
                    .map(|page| vec![page.source_id.clone()])
                    .unwrap_or_default();
                trace.push(WebResearchTraceStep {
                    route,
                    status: WebResearchTraceStatus::Succeeded,
                    source_ids,
                });
                if let Some(page) = observed.page {
                    research.record_opened_document(&page.final_url);
                    pages.record(page);
                }
                observed.observation
            })
        }
        WebToolRoute::Find { query } => {
            context.progress.emit(TuiRequestProgress::Finding);
            let route = WebToolRoute::Find {
                query: query.clone(),
            };
            let result =
                web_search_adapter::observe_find_in_page(pages.current(), &query, context.request);
            if let Ok(observation) = &result {
                trace.push(WebResearchTraceStep {
                    route,
                    status: WebResearchTraceStatus::Succeeded,
                    source_ids: observation_source_ids(observation),
                });
            }
            result
        }
    };
    // ureq owns each blocking transport call. The safe portable cancellation
    // boundary is therefore between Search/Open/Find transport steps.
    context.cancellation.check()?;
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

fn observation_source_ids(observation: &WebToolObservation) -> Vec<String> {
    match observation {
        WebToolObservation::Evidence(observation) => observation
            .grounding
            .iter()
            .map(|evidence| evidence.source_id.clone())
            .collect(),
        WebToolObservation::Terminal(answer) => answer
            .grounding
            .iter()
            .map(|evidence| evidence.source_id.clone())
            .collect(),
    }
}

pub(super) fn answer(
    observation: WebToolObservation,
    request: &str,
    cancellation: &RequestCancellationToken,
) -> Result<WebToolExecution, AppError> {
    let answer =
        web_search_adapter::answer_observation_with_cancel(observation, request, cancellation)?;
    Ok(WebToolExecution {
        response: answer.response,
        grounding: answer.grounding,
    })
}
