use super::*;

const HTML_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/ddg-html.html");
const LITE_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/ddg-lite.html");
const DRIFT_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/ddg-drift.html");
const ANTIBOT_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/ddg-antibot.html");
const HOSTILE_PAGE_FIXTURE: &str =
    include_str!("../../../tests/fixtures/web_search/page-hostile.html");
const MARKDOWN_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/page.md");
const RSS_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/feed-rss.xml");
const ATOM_FIXTURE: &str = include_str!("../../../tests/fixtures/web_search/feed-atom.xml");

include!("tests/browser_policy.rs");
include!("tests/search.rs");
include!("tests/open_find.rs");
