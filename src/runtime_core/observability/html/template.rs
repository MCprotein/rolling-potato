use std::fmt::Write;

const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; \
    img-src 'none'; script-src 'none'; connect-src 'none'; font-src 'none'; \
    object-src 'none'; media-src 'none'; frame-src 'none'; worker-src 'none'; \
    manifest-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #f4f1e8;
  --panel: #fffdf7;
  --text: #20221f;
  --muted: #62685f;
  --line: #c8c9bd;
  --accent: #235d48;
  --healthy: #17633d;
  --warning: #7a5200;
  --failed: #a02b2b;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
header, main, footer { width: min(72rem, calc(100% - 2rem)); margin-inline: auto; }
header { padding: 2.5rem 0 1.25rem; border-bottom: 2px solid var(--text); }
h1 { margin: 0 0 .4rem; font-size: clamp(1.7rem, 4vw, 2.8rem); letter-spacing: -.04em; }
h2 { margin: 0 0 .8rem; font-size: 1.15rem; }
p { margin: .35rem 0; }
.eyebrow { color: var(--accent); font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
.muted, footer { color: var(--muted); }
main { display: grid; gap: 1rem; padding: 1rem 0 2rem; }
section { padding: 1rem; background: var(--panel); border: 1px solid var(--line); }
.summary { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: .65rem; }
.metric { min-width: 0; padding: .8rem; border-left: 3px solid var(--accent); background: var(--bg); }
.metric strong { display: block; margin-top: .25rem; font-size: 1.15rem; overflow-wrap: anywhere; }
.status { font-weight: 700; }
.healthy { color: var(--healthy); }
.warning { color: var(--warning); }
.failed { color: var(--failed); }
.table-wrap { max-width: 100%; overflow-x: auto; }
table { width: 100%; border-collapse: collapse; white-space: nowrap; }
caption { padding: 0 0 .6rem; color: var(--muted); text-align: left; }
th, td { padding: .55rem .65rem; border-bottom: 1px solid var(--line); text-align: left; }
th { color: var(--muted); font-size: .85rem; }
.empty { padding: .8rem; border: 1px dashed var(--line); color: var(--muted); }
dl { display: grid; grid-template-columns: minmax(10rem, .4fr) 1fr; margin: 0; }
dt, dd { margin: 0; padding: .45rem 0; border-bottom: 1px solid var(--line); }
dt { color: var(--muted); }
dd { overflow-wrap: anywhere; }
footer { padding: 0 0 2rem; }
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #171a18;
    --panel: #202421;
    --text: #eceee9;
    --muted: #adb5aa;
    --line: #454d47;
    --accent: #70c69e;
    --healthy: #70c69e;
    --warning: #e1bb68;
    --failed: #f18a8a;
  }
}
@media (max-width: 48rem) {
  header, main, footer { width: min(100% - 1rem, 72rem); }
  header { padding-top: 1.5rem; }
  .summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  dl { grid-template-columns: 1fr; }
  dt { padding-bottom: 0; border-bottom: 0; }
  dd { padding-top: .15rem; }
}
@media (max-width: 30rem) {
  .summary { grid-template-columns: 1fr; }
  section { padding: .8rem; }
}
"#;

pub(super) fn render_document_start(html: &mut String, generated_at_ms: u128) {
    html.push_str("<!doctype html>\n<html lang=\"ko\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    writeln!(
        html,
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{}\">",
        CONTENT_SECURITY_POLICY
    )
    .expect("writing to String cannot fail");
    html.push_str("<title>rolling-potato monitor report</title>\n<style>");
    html.push_str(STYLE);
    html.push_str("</style>\n</head>\n<body>\n");
    write!(
        html,
        "<header><p class=\"eyebrow\">local monitor snapshot</p>\
         <h1>rolling-potato monitor report</h1>\
         <p>로컬 데이터만 읽어 만든 정적 report입니다.</p>\
         <p class=\"muted\">생성 시각: {} ms (Unix epoch) · data source: SQLite projection + canonical ledger</p>\
         </header>\n<main>\n",
        generated_at_ms
    )
    .expect("writing to String cannot fail");
}

pub(super) fn render_document_end(html: &mut String) {
    write!(
        html,
        "</main>\n<footer>rpotato {} · read-only · offline · redacted</footer>\n</body>\n</html>\n",
        env!("CARGO_PKG_VERSION")
    )
    .expect("writing to String cannot fail");
}
