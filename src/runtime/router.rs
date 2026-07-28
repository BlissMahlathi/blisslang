/// BlissLang Runtime Router
///
/// Holds the compiled route registry — maps URL path patterns to
/// either a pre-built static HTML string (static/hybrid pages) or
/// a template string that is rendered per-request (runtime pages).
///
/// Routes are registered at server startup and matched on every request.

use std::collections::HashMap;
use crate::runtime::request::match_pattern;

// ─── Route Kind ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RouteKind {
    /// Page is pre-rendered. Serve the HTML as-is on every request.
    Static { html: String },
    /// Page is rendered per-request. The HTML may contain {{query.x}} etc.
    Runtime { template: String },
}

// ─── Route Entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Route {
    /// URL pattern, e.g. "/" or "/users/:id" or "/blog/:slug"
    pub pattern: String,
    pub kind:    RouteKind,
    /// Optional page title for dynamic <title> injection
    pub title:   Option<String>,
}

// ─── Router ───────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a static page (served as-is on every request).
    pub fn add_static(&mut self, pattern: impl Into<String>, html: impl Into<String>, title: Option<String>) {
        self.routes.push(Route {
            pattern: pattern.into(),
            kind:    RouteKind::Static { html: html.into() },
            title,
        });
    }

    /// Register a runtime page (template rendered on every request).
    pub fn add_runtime(&mut self, pattern: impl Into<String>, template: impl Into<String>, title: Option<String>) {
        self.routes.push(Route {
            pattern: pattern.into(),
            kind:    RouteKind::Runtime { template: template.into() },
            title,
        });
    }

    /// Match an incoming URL path against registered routes.
    /// Returns the matched route and any extracted path params.
    /// Precedence: exact matches first, then pattern matches in registration order.
    pub fn resolve<'a>(&'a self, path: &str) -> Option<(&'a Route, HashMap<String, String>)> {
        // Clean the path
        let path = path.split('?').next().unwrap_or(path);
        let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };

        // 1st pass: exact match (fastest, most common case)
        for route in &self.routes {
            if !route.pattern.contains(':') && route.pattern == path {
                return Some((route, HashMap::new()));
            }
        }

        // 2nd pass: pattern match (routes with :param segments)
        for route in &self.routes {
            if route.pattern.contains(':') {
                if let Some(params) = match_pattern(&route.pattern, path) {
                    return Some((route, params));
                }
            }
        }

        // 3rd pass: try with trailing slash added/removed
        let alt = if path.ends_with('/') {
            path.trim_end_matches('/').to_string()
        } else {
            format!("{}/", path)
        };
        for route in &self.routes {
            if !route.pattern.contains(':') && route.pattern == alt {
                return Some((route, HashMap::new()));
            }
        }

        None
    }

    /// True if there are any runtime-mode routes registered.
    #[allow(dead_code)]
    pub fn has_runtime_routes(&self) -> bool {
        self.routes.iter().any(|r| matches!(r.kind, RouteKind::Runtime { .. }))
    }

    /// Count of each kind of route.
    pub fn stats(&self) -> (usize, usize) {
        let static_count  = self.routes.iter().filter(|r| matches!(r.kind, RouteKind::Static { .. })).count();
        let runtime_count = self.routes.iter().filter(|r| matches!(r.kind, RouteKind::Runtime { .. })).count();
        (static_count, runtime_count)
    }
}

// ─── Build Router from ProjectFiles ──────────────────────────────────────────

/// Build the runtime router from the project's compiled pages.
/// Called once at startup. Pages with `output: "runtime"` or `output: "hybrid"`
/// are registered as runtime routes; everything else is static.
pub fn build_router(
    pages: &HashMap<String, (String, crate::compiler::ast::OutputMode)>,
) -> Router {
    let mut router = Router::new();

    for (route, (html, output_mode)) in pages {
        let title = extract_title(html);
        match output_mode {
            crate::compiler::ast::OutputMode::Static => {
                router.add_static(route, html.clone(), title);
            }
            crate::compiler::ast::OutputMode::Runtime |
            crate::compiler::ast::OutputMode::Hybrid => {
                router.add_runtime(route, html.clone(), title);
            }
        }
    }

    router
}

/// Extract <title> content from an HTML string.
fn extract_title(html: &str) -> Option<String> {
    let start = html.find("<title>")? + 7;
    let end   = html[start..].find("</title>")? + start;
    Some(html[start..end].to_string())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_router() -> Router {
        let mut r = Router::new();
        r.add_static("/",         "<html>Home</html>",   Some("Home".to_string()));
        r.add_static("/about",    "<html>About</html>",  None);
        r.add_runtime("/users/:id", "<html>{{param.id}}</html>", None);
        r.add_runtime("/search",  "<html>{{query.q}}</html>",    None);
        r
    }

    #[test]
    fn test_exact_match_home() {
        let router = make_router();
        let (route, params) = router.resolve("/").unwrap();
        assert!(matches!(route.kind, RouteKind::Static { .. }));
        assert!(params.is_empty());
    }

    #[test]
    fn test_exact_match_about() {
        let router = make_router();
        let (route, params) = router.resolve("/about").unwrap();
        assert!(matches!(route.kind, RouteKind::Static { .. }));
        assert!(params.is_empty());
    }

    #[test]
    fn test_pattern_match_with_param() {
        let router = make_router();
        let (route, params) = router.resolve("/users/42").unwrap();
        assert!(matches!(route.kind, RouteKind::Runtime { .. }));
        assert_eq!(params.get("id"), Some(&"42".to_string()));
    }

    #[test]
    fn test_runtime_search_route() {
        let router = make_router();
        let (route, _) = router.resolve("/search").unwrap();
        assert!(matches!(route.kind, RouteKind::Runtime { .. }));
    }

    #[test]
    fn test_no_match_returns_none() {
        let router = make_router();
        assert!(router.resolve("/nonexistent").is_none());
    }

    #[test]
    fn test_trailing_slash_tolerance() {
        let router = make_router();
        // /about/ should resolve to /about
        let result = router.resolve("/about/");
        assert!(result.is_some());
    }

    #[test]
    fn test_has_runtime_routes() {
        let router = make_router();
        assert!(router.has_runtime_routes());

        let mut static_only = Router::new();
        static_only.add_static("/", "html", None);
        assert!(!static_only.has_runtime_routes());
    }

    #[test]
    fn test_stats() {
        let router = make_router();
        let (s, r) = router.stats();
        assert_eq!(s, 2); // home + about
        assert_eq!(r, 2); // users/:id + search
    }

    #[test]
    fn test_query_string_ignored_in_match() {
        let router = make_router();
        // Query string should be stripped before matching
        let result = router.resolve("/about?foo=bar");
        assert!(result.is_some());
    }

    #[test]
    fn test_extract_title() {
        let html = "<!DOCTYPE html><html><head><title>My Page</title></head></html>";
        assert_eq!(extract_title(html), Some("My Page".to_string()));
    }
}
