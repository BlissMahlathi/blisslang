/// BlissLang Runtime Request
///
/// A parsed HTTP/1.1 request. Used by the runtime server to pass
/// request context into page templates rendered on demand.
/// No external crate — pure std::io parsing.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;

// ─── HTTP Method ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Other(String),
}

impl Method {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET"     => Method::Get,
            "POST"    => Method::Post,
            "PUT"     => Method::Put,
            "PATCH"   => Method::Patch,
            "DELETE"  => Method::Delete,
            "HEAD"    => Method::Head,
            "OPTIONS" => Method::Options,
            other     => Method::Other(other.to_string()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Method::Get     => "GET",
            Method::Post    => "POST",
            Method::Put     => "PUT",
            Method::Patch   => "PATCH",
            Method::Delete  => "DELETE",
            Method::Head    => "HEAD",
            Method::Options => "OPTIONS",
            Method::Other(s)=> s.as_str(),
        }
    }
}

// ─── Runtime Request ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeRequest {
    pub method:      Method,
    pub path:        String,
    pub query:       HashMap<String, String>,
    pub headers:     HashMap<String, String>,
    pub body:        Option<String>,
    pub path_params: HashMap<String, String>,
}

#[allow(dead_code)]
impl RuntimeRequest {
    /// Parse an HTTP/1.1 request from a TCP stream.
    /// Returns None if the stream is empty or malformed.
    pub fn parse(reader: &mut BufReader<TcpStream>) -> Option<Self> {
        // Request line: GET /path?query HTTP/1.1
        let mut request_line = String::new();
        reader.read_line(&mut request_line).ok()?;
        let request_line = request_line.trim();
        if request_line.is_empty() { return None; }

        let mut parts = request_line.split_whitespace();
        let method_str = parts.next()?.to_string();
        let raw_path   = parts.next()?.to_string();

        let method = Method::from_str(&method_str);

        // Split path and query string
        let (path, query) = if let Some(idx) = raw_path.find('?') {
            let p = raw_path[..idx].to_string();
            let q = parse_query_string(&raw_path[idx+1..]);
            (p, q)
        } else {
            (raw_path.clone(), HashMap::new())
        };

        // Decode the path
        let path = url_decode(&path);

        // Read headers until blank line
        let mut headers = HashMap::new();
        let mut content_length = 0usize;

        loop {
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;
            let line = line.trim();
            if line.is_empty() { break; }

            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim().to_lowercase();
                let val = val.trim().to_string();
                if key == "content-length" {
                    content_length = val.parse().unwrap_or(0);
                }
                headers.insert(key, val);
            }
        }

        // Read body for POST/PUT/PATCH
        let body = if content_length > 0 && matches!(method, Method::Post | Method::Put | Method::Patch) {
            let mut buf = vec![0u8; content_length.min(65536)]; // cap at 64KB
            use std::io::Read;
            reader.read_exact(&mut buf).ok()?;
            Some(String::from_utf8_lossy(&buf).to_string())
        } else {
            None
        };

        Some(RuntimeRequest {
            method,
            path,
            query,
            headers,
            body,
            path_params: HashMap::new(),
        })
    }

    /// Get a query param by name.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(|s| s.as_str())
    }

    /// Get a request header by name (lowercase).
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|s| s.as_str())
    }

    /// Get a path param by name (set after route matching).
    pub fn param(&self, key: &str) -> Option<&str> {
        self.path_params.get(key).map(|s| s.as_str())
    }

    /// True if this looks like an asset request (CSS, JS, image, font).
    pub fn is_asset(&self) -> bool {
        let ext = self.path.rsplit('.').next().unwrap_or("");
        matches!(ext, "css"|"js"|"png"|"jpg"|"jpeg"|"gif"|"svg"|"webp"|
                      "woff"|"woff2"|"ttf"|"otf"|"ico"|"pdf"|"mp4"|"webm")
    }

    /// Build a context HashMap for template rendering.
    /// Flattens all request data into a `key → value` map that the
    /// runtime template engine can substitute into page HTML.
    pub fn to_context(&self) -> HashMap<String, String> {
        let mut ctx = HashMap::new();

        // Query params: query.name → value
        for (k, v) in &self.query {
            ctx.insert(format!("query.{}", k), v.clone());
        }

        // Headers: header.content-type → value
        for (k, v) in &self.headers {
            ctx.insert(format!("header.{}", k), v.clone());
        }

        // Path params: param.id → value
        for (k, v) in &self.path_params {
            ctx.insert(format!("param.{}", k), v.clone());
        }

        // Request metadata
        ctx.insert("request.method".to_string(), self.method.as_str().to_string());
        ctx.insert("request.path".to_string(),   self.path.clone());

        ctx
    }
}

// ─── Route Matcher ────────────────────────────────────────────────────────────

/// Match a URL path against a route pattern with `:param` segments.
/// Returns extracted path params if matched.
///
/// Pattern: "/users/:id/posts/:post_id"
/// Path:    "/users/42/posts/7"
/// Returns: Some({ "id": "42", "post_id": "7" })
pub fn match_pattern(pattern: &str, path: &str) -> Option<HashMap<String, String>> {
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts:    Vec<&str> = path.split('/').collect();

    if pattern_parts.len() != path_parts.len() {
        return None;
    }

    let mut params = HashMap::new();

    for (pat, seg) in pattern_parts.iter().zip(path_parts.iter()) {
        if let Some(name) = pat.strip_prefix(':') {
            // Dynamic segment — capture value
            params.insert(name.to_string(), seg.to_string());
        } else if *pat != *seg {
            return None;
        }
    }

    Some(params)
}

// ─── Query String Parser ──────────────────────────────────────────────────────

/// Parse `key=value&key2=value2` into a HashMap.
pub fn parse_query_string(qs: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in qs.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(url_decode(k), url_decode(v));
        } else if !pair.is_empty() {
            map.insert(url_decode(pair), String::new());
        }
    }
    map
}

/// Minimal URL decoder for common percent-encoded characters.
pub fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars  = s.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            let hex = format!("{}{}", h1, h2);
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push(h1);
                result.push(h2);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_string() {
        let q = parse_query_string("name=Bliss&city=Nkowankowa&page=1");
        assert_eq!(q.get("name"),  Some(&"Bliss".to_string()));
        assert_eq!(q.get("city"),  Some(&"Nkowankowa".to_string()));
        assert_eq!(q.get("page"),  Some(&"1".to_string()));
    }

    #[test]
    fn test_parse_empty_query() {
        let q = parse_query_string("");
        assert!(q.is_empty());
    }

    #[test]
    fn test_url_decode_plus() {
        assert_eq!(url_decode("hello+world"), "hello world");
    }

    #[test]
    fn test_url_decode_percent() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("Nkowankowa%2C+Limpopo"), "Nkowankowa, Limpopo");
    }

    #[test]
    fn test_match_pattern_exact() {
        let m = match_pattern("/about", "/about");
        assert!(m.is_some());
        assert!(m.unwrap().is_empty());
    }

    #[test]
    fn test_match_pattern_param() {
        let m = match_pattern("/users/:id", "/users/42");
        let params = m.unwrap();
        assert_eq!(params.get("id"), Some(&"42".to_string()));
    }

    #[test]
    fn test_match_pattern_multi_param() {
        let m = match_pattern("/users/:id/posts/:slug", "/users/5/posts/hello-world");
        let params = m.unwrap();
        assert_eq!(params.get("id"),   Some(&"5".to_string()));
        assert_eq!(params.get("slug"), Some(&"hello-world".to_string()));
    }

    #[test]
    fn test_match_pattern_no_match() {
        assert!(match_pattern("/users/:id", "/posts/42").is_none());
        assert!(match_pattern("/a/b",       "/a/b/c").is_none());
    }

    #[test]
    fn test_request_context() {
        let mut req = RuntimeRequest {
            method:      Method::Get,
            path:        "/search".to_string(),
            query:       parse_query_string("q=BlissLang&page=2"),
            headers:     { let mut h = HashMap::new(); h.insert("accept".to_string(), "text/html".to_string()); h },
            body:        None,
            path_params: HashMap::new(),
        };
        let ctx = req.to_context();
        assert_eq!(ctx.get("query.q"),      Some(&"BlissLang".to_string()));
        assert_eq!(ctx.get("query.page"),   Some(&"2".to_string()));
        assert_eq!(ctx.get("header.accept"),Some(&"text/html".to_string()));
        assert_eq!(ctx.get("request.method"), Some(&"GET".to_string()));
    }

    #[test]
    fn test_method_parsing() {
        assert_eq!(Method::from_str("GET"),    Method::Get);
        assert_eq!(Method::from_str("POST"),   Method::Post);
        assert_eq!(Method::from_str("DELETE"), Method::Delete);
    }
}
