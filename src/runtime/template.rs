/// BlissLang Runtime Template Engine
///
/// Substitutes `{{expr}}` placeholders in pre-rendered HTML at request time.
/// This is the bridge between static compilation and dynamic content for
/// pages with `output: "runtime"`.
///
/// Supported expressions:
///   {{query.name}}         — URL query param
///   {{param.id}}           — path param  (/users/:id)
///   {{header.accept}}      — request header
///   {{request.method}}     — GET / POST / etc.
///   {{request.path}}       — current URL path
///   {{env.APP_VERSION}}    — environment variable (if allowed by security config)
///   {{bliss.timestamp}}    — server-generated values (timestamp, nonce)
///
/// Expressions that can't be resolved at request time are left as
/// empty string (not as the raw placeholder) — blank is safer than
/// leaking template syntax into the browser.

use std::collections::HashMap;

// ─── Template Value ───────────────────────────────────────────────────────────

/// A resolved or unresolvable template expression result.
#[derive(Debug, Clone)]
pub enum TemplateValue {
    Str(String),
    Missing,
}

impl TemplateValue {
    pub fn as_str(&self) -> &str {
        match self {
            TemplateValue::Str(s) => s.as_str(),
            TemplateValue::Missing => "",
        }
    }
}

// ─── Template Engine ──────────────────────────────────────────────────────────

pub struct TemplateEngine {
    /// All available values: "query.name" → "Bliss", "param.id" → "42", etc.
    context: HashMap<String, String>,
    /// Additional static values injected at template creation time (e.g. build metadata)
    statics: HashMap<String, String>,
}

impl TemplateEngine {
    pub fn new(context: HashMap<String, String>) -> Self {
        let mut statics = HashMap::new();

        // Built-in bliss.* values
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        statics.insert("bliss.timestamp".to_string(), ts.to_string());
        statics.insert("bliss.version".to_string(),   "0.6".to_string());

        Self { context, statics }
    }

    /// Substitute all `{{expr}}` placeholders in an HTML string.
    /// Runs in a single pass — O(n) in the size of the HTML.
    pub fn render(&self, html: &str) -> String {
        if !html.contains("{{") {
            return html.to_string(); // fast path — no placeholders
        }

        let mut result = String::with_capacity(html.len());
        let mut chars  = html.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' && chars.peek() == Some(&'{') {
                chars.next(); // consume second {

                // Collect expression until }}
                let mut expr   = String::new();
                let mut closed = false;

                loop {
                    match chars.next() {
                        Some('}') if chars.peek() == Some(&'}') => {
                            chars.next(); // consume second }
                            closed = true;
                            break;
                        }
                        Some(c) => expr.push(c),
                        None => break,
                    }
                }

                if closed {
                    let resolved = self.resolve(expr.trim());
                    result.push_str(resolved.as_str());
                } else {
                    // Unclosed — emit as-is
                    result.push_str("{{");
                    result.push_str(&expr);
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Resolve a single expression string.
    pub fn resolve(&self, expr: &str) -> TemplateValue {
        // Check request context (query.*, param.*, header.*, request.*)
        if let Some(v) = self.context.get(expr) {
            return TemplateValue::Str(v.clone());
        }

        // Check built-in bliss.* values
        if let Some(v) = self.statics.get(expr) {
            return TemplateValue::Str(v.clone());
        }

        // env.VAR_NAME — safe subset of environment variables
        if let Some(var_name) = expr.strip_prefix("env.") {
            // Only allow uppercase alphanumeric + underscore variable names
            // and only variables explicitly prefixed BLISS_ or APP_ for safety
            if (var_name.starts_with("BLISS_") || var_name.starts_with("APP_"))
                && var_name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                if let Ok(val) = std::env::var(var_name) {
                    return TemplateValue::Str(val);
                }
            }
        }

        TemplateValue::Missing
    }

    /// Add extra values to the context (e.g. page-level metadata).
    #[allow(dead_code)]
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.context.insert(key.into(), value.into());
    }
}

// ─── HTML attribute injection ─────────────────────────────────────────────────

/// Inject server-side data attributes into HTML elements marked with
/// `data-bliss-runtime="true"`. Used for elements whose content depends
/// on request context, so the browser gets the right value immediately.
#[allow(dead_code)]
pub fn inject_runtime_attrs(html: &str, ctx: &HashMap<String, String>) -> String {
    let engine = TemplateEngine::new(ctx.clone());
    engine.render(html)
}

// ─── Runtime page pre-processor ───────────────────────────────────────────────

/// Prepare a page's compiled HTML template for runtime serving.
/// Strips the outer <html>/<head>/<body> shell so we can re-wrap it
/// with per-request data injected into the head.
/// Returns (head_html, body_html) tuple.
#[allow(dead_code)]
pub fn split_page_template(html: &str) -> (String, String) {
    let head = extract_between(html, "<head>", "</head>")
        .unwrap_or_default()
        .to_string();
    let body = extract_between(html, "<body>", "</body>")
        .unwrap_or_default()
        .to_string();
    (head, body)
}

/// Reassemble head + body into a full page, injecting runtime meta tags.
#[allow(dead_code)]
pub fn assemble_runtime_page(
    head:  &str,
    body:  &str,
    lang:  &str,
    extra_head: &str,
) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n{}\n{}</head>\n<body>\n{}\n</body>\n</html>\n",
        lang, head, extra_head, body
    )
}

#[allow(dead_code)]
fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_pos = s.find(start)? + start.len();
    let end_pos   = s[start_pos..].find(end)? + start_pos;
    Some(&s[start_pos..end_pos])
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn test_no_placeholders_fast_path() {
        let engine = TemplateEngine::new(ctx(&[]));
        let html = "<h1>Hello World</h1>";
        assert_eq!(engine.render(html), html);
    }

    #[test]
    fn test_query_substitution() {
        let engine = TemplateEngine::new(ctx(&[
            ("query.name", "Bliss"),
            ("query.city", "Nkowankowa"),
        ]));
        let html = "<p>Hello {{query.name}} from {{query.city}}</p>";
        assert_eq!(engine.render(html), "<p>Hello Bliss from Nkowankowa</p>");
    }

    #[test]
    fn test_param_substitution() {
        let engine = TemplateEngine::new(ctx(&[("param.id", "42")]));
        let html = "<h1>User {{param.id}}</h1>";
        assert_eq!(engine.render(html), "<h1>User 42</h1>");
    }

    #[test]
    fn test_missing_expr_becomes_empty() {
        let engine = TemplateEngine::new(ctx(&[]));
        let html = "<p>{{query.missing}}</p>";
        assert_eq!(engine.render(html), "<p></p>");
    }

    #[test]
    fn test_bliss_timestamp_present() {
        let engine = TemplateEngine::new(ctx(&[]));
        let html = "<p>{{bliss.timestamp}}</p>";
        let out  = engine.render(html);
        // Timestamp should be non-empty digits
        let inner = out.trim_start_matches("<p>").trim_end_matches("</p>");
        assert!(!inner.is_empty());
        assert!(inner.chars().all(|c| c.is_ascii_digit()), "Expected numeric timestamp, got: {}", inner);
    }

    #[test]
    fn test_multiple_substitutions_one_pass() {
        let engine = TemplateEngine::new(ctx(&[
            ("query.q",    "BlissLang"),
            ("param.page", "2"),
            ("request.method", "GET"),
        ]));
        let html = "<title>{{query.q}} - page {{param.page}} via {{request.method}}</title>";
        assert_eq!(engine.render(html), "<title>BlissLang - page 2 via GET</title>");
    }

    #[test]
    fn test_split_page_template() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body><h1>Hello</h1></body></html>";
        let (head, body) = split_page_template(html);
        assert_eq!(head, "<title>Test</title>");
        assert_eq!(body, "<h1>Hello</h1>");
    }

    #[test]
    fn test_env_var_safe_prefix_only() {
        // BLISS_ prefix should be allowed
        std::env::set_var("BLISS_APP_NAME", "TestApp");
        let engine = TemplateEngine::new(ctx(&[]));
        let val = engine.resolve("env.BLISS_APP_NAME");
        assert_eq!(val.as_str(), "TestApp");
        std::env::remove_var("BLISS_APP_NAME");

        // Random env vars NOT allowed
        let val2 = engine.resolve("env.PATH");
        assert_eq!(val2.as_str(), ""); // Missing → empty string
    }
}
