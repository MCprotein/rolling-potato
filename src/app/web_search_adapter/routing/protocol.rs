use super::super::research::WebResearchStep;

pub(crate) fn route_tool_request(request: &str) -> Option<WebResearchStep> {
    let request = request.trim();
    if let Some(query) = request.strip_prefix("/search ") {
        return nonempty(query).map(|query| WebResearchStep::Search {
            query: query.to_string(),
        });
    }
    if let Some(url) = request.strip_prefix("/open ") {
        return nonempty(url).map(|url| WebResearchStep::Open {
            url: url.to_string(),
        });
    }
    if let Some(query) = request.strip_prefix("/find ") {
        return nonempty(query).map(|query| WebResearchStep::Find {
            query: query.to_string(),
        });
    }
    None
}

pub(crate) fn parse_agent_web_tool(response: &str) -> Option<WebResearchStep> {
    const MAX_AGENT_TOOL_INPUT_CHARS: usize = 512;

    let lines = response
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() || lines.len() > 2 {
        return None;
    }
    let mut tool = None;
    let mut input = None;
    for line in lines {
        let (label, value) = line.split_once(':')?;
        match normalized_protocol_label(label).as_str() {
            "webtool" if tool.is_none() => tool = nonempty(value),
            "webinput" if input.is_none() => input = nonempty(value),
            _ => return None,
        }
    }
    let input = input?;
    if input.contains(['\r', '\n']) || input.chars().count() > MAX_AGENT_TOOL_INPUT_CHARS {
        return None;
    }
    match tool.unwrap_or("search").to_ascii_lowercase().as_str() {
        "search" => Some(WebResearchStep::Search {
            query: input.to_string(),
        }),
        "open" => Some(WebResearchStep::Open {
            url: input.to_string(),
        }),
        "find" => Some(WebResearchStep::Find {
            query: input.to_string(),
        }),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn parse_agent_web_tool_for_request(
    response: &str,
    current_request: &str,
) -> Option<WebResearchStep> {
    parse_agent_web_tool_for_user_context(response, current_request, &[])
}

pub(crate) fn parse_agent_web_tool_for_user_context(
    response: &str,
    current_request: &str,
    prior_user_requests: &[&str],
) -> Option<WebResearchStep> {
    if conversational_progress_followup(current_request) {
        return None;
    }
    let step = parse_agent_web_tool(response)?;
    match step {
        WebResearchStep::Search { query } => {
            super::query::contextualize_search_input(&query, current_request, prior_user_requests)
                .map(|query| WebResearchStep::Search { query })
        }
        step => literal_projection(step.input(), current_request).then_some(step),
    }
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized_protocol_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn conversational_progress_followup(request: &str) -> bool {
    let compact = request
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace() && !character.is_ascii_punctuation())
        .collect::<String>();
    if compact.is_empty() {
        return true;
    }
    if compact.chars().count() > 16 || has_explicit_web_intent(&compact) {
        return false;
    }
    ["왜", "뭐", "뭔", "무슨", "그래서", "어떻게", "어디까지"]
        .iter()
        .any(|prefix| compact.starts_with(prefix))
        || ["하고있", "하는중", "검색중", "되고있", "진행중"]
            .iter()
            .any(|signal| compact.contains(signal))
}

fn has_explicit_web_intent(request: &str) -> bool {
    ["검색", "찾아", "웹", "인터넷", "search", "browse", "web"]
        .iter()
        .any(|signal| request.contains(signal))
}

fn literal_projection(input: &str, current_request: &str) -> bool {
    let input = input.trim().to_lowercase();
    let current_request = current_request.trim().to_lowercase();
    !input.is_empty() && current_request.contains(&input)
}
