/// BlissLang Dev Server — v0.2
///
/// Pure Rust standard library HTTP server.
/// No axum. No tokio. No hyper. No external HTTP crate.
///
/// Built on:
///   std::net::TcpListener   — accepts connections
///   std::thread             — one thread per connection (thread pool)
///   std::sync::Arc<RwLock>  — shared page state, hot-reloaded safely
///
/// Supports:
///   • Serving multiple routes (one per .page file)
///   • Live reload via hot reload header injection
///   • Static assets from /assets/ path
///   • Proper HTTP/1.1 response headers
///   • Concurrent connections via thread pool

use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

// ─── Shared Server State ──────────────────────────────────────────────────────

/// The pages map is shared across all threads and updated on hot reload.
/// Route → HTML content.
pub type PageMap = Arc<RwLock<HashMap<String, String>>>;

// ─── Server Config ────────────────────────────────────────────────────────────

pub struct ServerConfig {
    pub port:       u16,
    pub host:       String,
    pub hot_reload: bool,
    pub project:    String,
    pub threads:    usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port:       8080,
            host:       "127.0.0.1".to_string(),
            hot_reload: true,
            project:    ".".to_string(),
            threads:    4,
        }
    }
}

// ─── HTTP Request ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct HttpRequest {
    method:  String,
    path:    String,
    version: String,
    headers: HashMap<String, String>,
}

impl HttpRequest {
    fn parse(reader: &mut BufReader<TcpStream>) -> Option<Self> {
        // Read the request line: GET /path HTTP/1.1
        let mut request_line = String::new();
        reader.read_line(&mut request_line).ok()?;
        let request_line = request_line.trim();

        let mut parts = request_line.split_whitespace();
        let method  = parts.next()?.to_string();
        let path    = parts.next()?.to_string();
        let version = parts.next().unwrap_or("HTTP/1.0").to_string();

        // Read headers until blank line
        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;
            let line = line.trim();
            if line.is_empty() { break; }
            if let Some((key, val)) = line.split_once(':') {
                headers.insert(
                    key.trim().to_lowercase(),
                    val.trim().to_string()
                );
            }
        }

        Some(HttpRequest { method, path, version, headers })
    }

    /// Clean path: strip query string and fragment, decode %xx
    fn clean_path(&self) -> String {
        let path = self.path.split('?').next().unwrap_or("/");
        let path = path.split('#').next().unwrap_or("/");
        // Basic URL decode for common cases
        path.replace("%20", " ")
            .replace("%2F", "/")
    }
}

// ─── HTTP Response ────────────────────────────────────────────────────────────

struct HttpResponse {
    status:  u16,
    reason:  &'static str,
    headers: Vec<(String, String)>,
    body:    Vec<u8>,
}

impl HttpResponse {
    fn html(status: u16, reason: &'static str, body: String) -> Self {
        let bytes = body.into_bytes();
        Self {
            status,
            reason,
            headers: vec![
                ("Content-Type".into(),   "text/html; charset=utf-8".into()),
                ("Content-Length".into(), bytes.len().to_string()),
                ("Connection".into(),     "close".into()),
                ("X-Powered-By".into(),   "BlissLang/0.2".into()),
                ("Cache-Control".into(),  "no-cache".into()),
            ],
            body: bytes,
        }
    }

    fn css(body: String) -> Self {
        let bytes = body.into_bytes();
        Self {
            status: 200,
            reason: "OK",
            headers: vec![
                ("Content-Type".into(),   "text/css; charset=utf-8".into()),
                ("Content-Length".into(), bytes.len().to_string()),
                ("Connection".into(),     "close".into()),
                ("Cache-Control".into(),  "no-cache".into()),
            ],
            body: bytes,
        }
    }

    fn js(body: String) -> Self {
        let bytes = body.into_bytes();
        Self {
            status: 200,
            reason: "OK",
            headers: vec![
                ("Content-Type".into(),   "application/javascript; charset=utf-8".into()),
                ("Content-Length".into(), bytes.len().to_string()),
                ("Connection".into(),     "close".into()),
                ("Cache-Control".into(),  "no-cache".into()),
            ],
            body: bytes,
        }
    }

    fn sse() -> Self {
        // Server-Sent Events stream for hot reload
        Self {
            status: 200,
            reason: "OK",
            headers: vec![
                ("Content-Type".into(),      "text/event-stream".into()),
                ("Cache-Control".into(),     "no-cache".into()),
                ("Connection".into(),        "keep-alive".into()),
                ("X-Accel-Buffering".into(), "no".into()),
            ],
            body: Vec::new(),
        }
    }

    fn send(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        // Status line
        let status_line = format!("HTTP/1.1 {} {}\r\n", self.status, self.reason);
        stream.write_all(status_line.as_bytes())?;

        // Headers
        for (key, val) in &self.headers {
            stream.write_all(format!("{}: {}\r\n", key, val).as_bytes())?;
        }
        stream.write_all(b"\r\n")?;

        // Body
        if !self.body.is_empty() {
            stream.write_all(&self.body)?;
        }

        stream.flush()?;
        Ok(())
    }
}

// ─── Hot Reload Signal ────────────────────────────────────────────────────────

/// Shared flag: when set to true, the hot reload SSE endpoint sends a reload event.
pub type ReloadSignal = Arc<std::sync::atomic::AtomicBool>;

// ─── Request Handler ──────────────────────────────────────────────────────────

fn handle_connection(
    mut stream: TcpStream,
    pages:      PageMap,
    reload:     ReloadSignal,
    project:    String,
    hot_reload: bool,
) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let req = match HttpRequest::parse(&mut reader) {
        Some(r) => r,
        None    => return,
    };

    let path = req.clean_path();

    // ── Special routes ─────────────────────────────────────────────────

    // Hot reload SSE endpoint
    if path == "/_bliss/reload" && hot_reload {
        handle_hot_reload_sse(&mut stream, reload);
        return;
    }

    // BlissLang runtime JS
    if path == "/_bliss/runtime.js" {
        let response = HttpResponse::js(runtime_js(hot_reload));
        response.send(&mut stream).ok();
        return;
    }

    // Static assets
    if path.starts_with("/assets/") {
        handle_static_asset(&mut stream, &path, &project);
        return;
    }

    // ── Page routes ────────────────────────────────────────────────────

    let pages_read = pages.read().unwrap();

    // Try exact route match
    let html = pages_read.get(&path)
        // Try with trailing slash removed
        .or_else(|| {
            if path.ends_with('/') && path.len() > 1 {
                pages_read.get(path.trim_end_matches('/'))
            } else {
                None
            }
        })
        // Try route + index (e.g. /about → /about or /)
        .or_else(|| pages_read.get("/"))
        .cloned();

    drop(pages_read);

    match html {
        Some(mut content) => {
            // Inject hot reload script before </body>
            if hot_reload {
                content = inject_hot_reload(content);
            }
            let response = HttpResponse::html(200, "OK", content);
            response.send(&mut stream).ok();
            println!("  {} {} {}", "200".green(), req.method.dimmed(), path.cyan());
        }
        None => {
            let body = not_found_page(&path);
            let response = HttpResponse::html(404, "Not Found", body);
            response.send(&mut stream).ok();
            println!("  {} {} {}", "404".yellow(), req.method.dimmed(), path);
        }
    }
}

// ─── Hot Reload SSE Handler ───────────────────────────────────────────────────

fn handle_hot_reload_sse(stream: &mut TcpStream, reload: ReloadSignal) {
    use std::sync::atomic::Ordering;

    // Send SSE headers
    let headers = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        X-Accel-Buffering: no\r\n\
        \r\n";
    if stream.write_all(headers.as_bytes()).is_err() { return; }

    // Send initial heartbeat
    if stream.write_all(b": connected\n\n").is_err() { return; }
    stream.flush().ok();

    // Poll for reload signal
    loop {
        thread::sleep(Duration::from_millis(200));

        if reload.load(Ordering::Relaxed) {
            reload.store(false, Ordering::Relaxed);
            // Send reload event
            if stream.write_all(b"event: reload\ndata: {}\n\n").is_err() { break; }
            if stream.flush().is_err() { break; }
        }

        // Heartbeat every 10 seconds to keep connection alive
        if stream.write_all(b": heartbeat\n\n").is_err() { break; }
        if stream.flush().is_err() { break; }

        thread::sleep(Duration::from_secs(9));
    }
}

// ─── Static Asset Handler ─────────────────────────────────────────────────────

fn handle_static_asset(stream: &mut TcpStream, path: &str, project: &str) {
    // Map /assets/foo.png → project/Assets/foo.png
    let relative = path.trim_start_matches("/assets/");
    let file_path = format!("{}/Assets/{}", project, relative);

    match fs::read(&file_path) {
        Ok(bytes) => {
            let mime = mime_type_for(relative);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                mime, bytes.len()
            );
            stream.write_all(response.as_bytes()).ok();
            stream.write_all(&bytes).ok();
            stream.flush().ok();
        }
        Err(_) => {
            let response = HttpResponse::html(404, "Not Found",
                format!("<h1>Asset not found: {}</h1>", path));
            response.send(stream).ok();
        }
    }
}

fn mime_type_for(filename: &str) -> &'static str {
    let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "html"       => "text/html; charset=utf-8",
        "css"        => "text/css; charset=utf-8",
        "js"         => "application/javascript; charset=utf-8",
        "json"       => "application/json",
        "png"        => "image/png",
        "jpg"|"jpeg" => "image/jpeg",
        "gif"        => "image/gif",
        "svg"        => "image/svg+xml",
        "webp"       => "image/webp",
        "ico"        => "image/x-icon",
        "woff"       => "font/woff",
        "woff2"      => "font/woff2",
        "ttf"        => "font/ttf",
        "otf"        => "font/otf",
        "mp4"        => "video/mp4",
        "webm"       => "video/webm",
        "mp3"        => "audio/mpeg",
        "wav"        => "audio/wav",
        "pdf"        => "application/pdf",
        _            => "application/octet-stream",
    }
}

// ─── HTML Injections ──────────────────────────────────────────────────────────

/// Inject the BlissLang runtime JS + hot reload client before </body>
fn inject_hot_reload(html: String) -> String {
    let script = r#"<script src="/_bliss/runtime.js"></script>
</body>"#;

    if html.contains("</body>") {
        html.replace("</body>", script)
    } else {
        format!("{}\n<script src=\"/_bliss/runtime.js\"></script>", html)
    }
}

// ─── Runtime JS ──────────────────────────────────────────────────────────────

/// The BlissLang runtime — served at /_bliss/runtime.js
/// Handles: scroll animations, ShowIf reactivity, hot reload client
fn runtime_js(hot_reload: bool) -> String {
    let hot_reload_client = if hot_reload {
        r#"
// ── Hot Reload Client ──────────────────────────────────────────────
(function() {
    var es = new EventSource('/_bliss/reload');
    es.addEventListener('reload', function() {
        console.log('[BlissLang] Hot reload — refreshing...');
        window.location.reload();
    });
    es.onerror = function() {
        // Server went away — retry quietly
        setTimeout(function() {
            es.close();
        }, 1000);
    };
    console.log('[BlissLang] Hot reload connected');
})();
"#
    } else { "" };

    format!(r#"
// BlissLang Runtime v0.2
// Built-in — no npm, no external libraries
(function() {{
    'use strict';

    // ── Scroll-triggered animations ─────────────────────────────────────
    function initAnimations() {{
        var els = document.querySelectorAll('[data-animate]');
        if (!els.length) return;

        if (!('IntersectionObserver' in window)) {{
            // Fallback: show all immediately
            els.forEach(function(el) {{ el.style.opacity = '1'; }});
            return;
        }}

        var observer = new IntersectionObserver(function(entries) {{
            entries.forEach(function(entry) {{
                if (!entry.isIntersecting) return;
                var el      = entry.target;
                var name    = el.dataset.animate;
                var delay   = el.dataset.animateDelay    || '0ms';
                var dur     = el.dataset.animateDuration || '600ms';
                var trigger = el.dataset.animateTrigger  || 'scroll';
                var ease    = el.dataset.animateEasing   || 'ease';

                if (trigger === 'scroll' || trigger === 'load') {{
                    el.style.animationName      = 'bliss-' + name;
                    el.style.animationDuration  = dur;
                    el.style.animationDelay     = delay;
                    el.style.animationFillMode  = 'both';
                    el.style.animationTimingFunction = ease;
                    el.style.opacity            = '';
                    el.classList.add('bliss-visible');
                    observer.unobserve(el);
                }}
            }});
        }}, {{ threshold: 0.15 }});

        els.forEach(function(el) {{
            var trigger = el.dataset.animateTrigger || 'scroll';
            if (trigger === 'load') {{
                // Fire immediately on load
                var name  = el.dataset.animate;
                var delay = el.dataset.animateDelay    || '0ms';
                var dur   = el.dataset.animateDuration || '600ms';
                el.style.animationName      = 'bliss-' + name;
                el.style.animationDuration  = dur;
                el.style.animationDelay     = delay;
                el.style.animationFillMode  = 'both';
                el.style.opacity            = '';
            }} else {{
                observer.observe(el);
            }}
        }});
    }}

    // ── ShowIf reactivity ────────────────────────────────────────────────
    function initShowIf() {{
        var showEls = document.querySelectorAll('[data-showif]');
        showEls.forEach(function(el) {{
            // Initially hidden — JS will show based on condition
            // Full signal reactivity comes in v0.3
            el.style.display = 'block';
        }});
    }}

    // ── Reactive elements ────────────────────────────────────────────────
    function initReactive() {{
        // Placeholder — full signal system in v0.3
        // Marks reactive elements for the state engine to manage
        var reactEls = document.querySelectorAll('[data-reactive]');
        reactEls.forEach(function(el) {{
            el.setAttribute('data-bliss-reactive', 'pending');
        }});
    }}

    // ── Hover animations ─────────────────────────────────────────────────
    function initHoverAnimations() {{
        var els = document.querySelectorAll('[data-animate-trigger="hover"]');
        els.forEach(function(el) {{
            var name = el.dataset.animate;
            var dur  = el.dataset.animateDuration || '300ms';
            el.addEventListener('mouseenter', function() {{
                el.style.animationName     = 'bliss-' + name;
                el.style.animationDuration = dur;
                el.style.animationFillMode = 'both';
                el.style.opacity           = '';
            }});
        }});
    }}

    // ── Click animations ──────────────────────────────────────────────────
    function initClickAnimations() {{
        var els = document.querySelectorAll('[data-animate-trigger="click"]');
        els.forEach(function(el) {{
            var name = el.dataset.animate;
            var dur  = el.dataset.animateDuration || '400ms';
            el.addEventListener('click', function() {{
                // Reset and replay
                el.style.animationName = 'none';
                el.offsetHeight; // reflow
                el.style.animationName     = 'bliss-' + name;
                el.style.animationDuration = dur;
                el.style.animationFillMode = 'both';
            }});
        }});
    }}

    // ── BlissGeo animated shapes ──────────────────────────────────────────
    function initGeoAnimations() {{
        var shapes = document.querySelectorAll('[data-geo-animate]');
        shapes.forEach(function(shape) {{
            var anim = shape.dataset.geoAnimate;
            var dur  = shape.dataset.geoDuration || '2s';
            var rep  = shape.dataset.geoRepeat   || 'infinite';

            shape.style.transformOrigin = 'center';
            shape.style.transformBox    = 'fill-box';
            shape.style.animation       = 'bliss-geo-' + anim + ' ' + dur + ' ' + rep + ' linear';
        }});
    }}

    // ── Init all ──────────────────────────────────────────────────────────
    if (document.readyState === 'loading') {{
        document.addEventListener('DOMContentLoaded', function() {{
            initAnimations();
            initShowIf();
            initReactive();
            initHoverAnimations();
            initClickAnimations();
            initGeoAnimations();
        }});
    }} else {{
        initAnimations();
        initShowIf();
        initReactive();
        initHoverAnimations();
        initClickAnimations();
        initGeoAnimations();
    }}

    // Expose BlissLang runtime API globally for v0.3 state system
    window.__bliss = {{
        version:  '0.2',
        signals:  {{}},
        navigate: function(url) {{ window.location.href = url; }},
        reload:   function() {{ window.location.reload(); }}
    }};

    console.log('%c BlissLang v0.2 %c Runtime ready ',
        'background:#E94560;color:#fff;padding:2px 6px;border-radius:3px 0 0 3px;font-weight:bold',
        'background:#1A1A2E;color:#fff;padding:2px 6px;border-radius:0 3px 3px 0'
    );
}})();
{}
"#, hot_reload_client)
}

// ─── 404 Page ─────────────────────────────────────────────────────────────────

fn not_found_page(path: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>404 — BlissLang</title>
    <script src="https://cdn.tailwindcss.com"></script>
</head>
<body class="bg-slate-900 min-h-screen flex items-center justify-center">
    <div class="text-center">
        <p class="text-8xl font-bold text-red-500 mb-4">404</p>
        <h1 class="text-2xl font-bold text-white mb-2">Page not found</h1>
        <p class="text-slate-400 mb-8">No .page file maps to route: <code class="text-red-400">{}</code></p>
        <a href="/" class="px-6 py-3 bg-red-500 text-white rounded-lg hover:bg-red-600">
            Go Home
        </a>
        <p class="text-slate-600 text-sm mt-8">BlissLang v0.2 — Build websites section by section</p>
    </div>
</body>
</html>"#, path)
}

// ─── Thread Pool ──────────────────────────────────────────────────────────────

/// Simple fixed-size thread pool.
/// Each incoming connection is dispatched to a worker thread.
struct ThreadPool {
    workers:  Vec<Worker>,
    sender:   std::sync::mpsc::Sender<Job>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    fn new(size: usize) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<Job>();
        let receiver = Arc::new(std::sync::Mutex::new(receiver));

        let workers = (0..size)
            .map(|id| Worker::new(id, Arc::clone(&receiver)))
            .collect();

        Self { workers, sender }
    }

    fn execute<F: FnOnce() + Send + 'static>(&self, job: F) {
        self.sender.send(Box::new(job)).ok();
    }
}

struct Worker {
    _id:    usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Job>>>) -> Self {
        let thread = thread::spawn(move || {
            loop {
                let job = {
                    let lock = receiver.lock().unwrap();
                    lock.recv()
                };
                match job {
                    Ok(job) => job(),
                    Err(_)  => break, // channel closed — shut down
                }
            }
        });
        Self { _id: id, thread: Some(thread) }
    }
}

// ─── Hot Reload File Watcher ──────────────────────────────────────────────────

/// Watch the project directory for .page/.section/.div changes.
/// When a change is detected, rebuild pages and signal reload.
pub fn start_watcher(
    project:    String,
    pages:      PageMap,
    reload:     ReloadSignal,
    rebuild_fn: impl Fn(&str) -> HashMap<String, String> + Send + 'static,
) {
    use notify::{Watcher, RecursiveMode, Config};
    use notify::recommended_watcher;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Instant;

    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();

        let mut watcher = match recommended_watcher(move |res| {
            let _ = tx.send(res);
        }) {
            Ok(w)  => w,
            Err(e) => {
                eprintln!("{} File watcher failed to start: {}", "⚠".yellow(), e);
                return;
            }
        };

        if let Err(e) = watcher.watch(Path::new(&project), RecursiveMode::Recursive) {
            eprintln!("{} Cannot watch {}: {}", "⚠".yellow(), project, e);
            return;
        }

        println!("  {} Watching {} for changes...", "👁".cyan(), project.dimmed());

        let mut last_rebuild = Instant::now();
        let debounce = Duration::from_millis(300);

        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(Ok(event)) => {
                    // Debounce — don't rebuild more than once per 300ms
                    if last_rebuild.elapsed() < debounce {
                        continue;
                    }

                    // Only rebuild on BlissLang source file changes
                    let relevant = event.paths.iter().any(|p| {
                        matches!(
                            p.extension().and_then(|e| e.to_str()),
                            Some("page"|"section"|"div"|"article"|"state"|"animation")
                        )
                    });

                    if !relevant { continue; }

                    let changed_files: Vec<String> = event.paths.iter()
                        .map(|p| p.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("?")
                            .to_string())
                        .collect();

                    print!("\n  {} {} changed — rebuilding... ",
                        "↺".yellow().bold(),
                        changed_files.join(", ").cyan()
                    );

                    // Rebuild
                    let new_pages = rebuild_fn(&project);
                    let count = new_pages.len();

                    {
                        let mut pages_write = pages.write().unwrap();
                        *pages_write = new_pages;
                    }

                    println!("{} ({} pages)", "done".green(), count);

                    // Signal hot reload
                    reload.store(true, Ordering::Relaxed);
                    last_rebuild = Instant::now();
                }
                Ok(Err(e)) => {
                    eprintln!("{} Watch error: {}", "⚠".yellow(), e);
                }
                Err(_) => {
                    // Timeout — just keep looping
                }
            }
        }
    });
}

// ─── Public: Static runtime JS (for build command) ───────────────────────────

/// Returns the runtime JS without hot reload — for static builds.
pub fn runtime_js_static() -> String {
    runtime_js(false)
}

// ─── Public: Start Dev Server ─────────────────────────────────────────────────

/// Start the BlissLang dev server.
/// Blocks the calling thread (run this on the main thread).
pub fn start(
    config:     ServerConfig,
    pages:      HashMap<String, String>,
    rebuild_fn: impl Fn(&str) -> HashMap<String, String> + Send + 'static,
) {
    use std::sync::atomic::AtomicBool;

    let addr = format!("{}:{}", config.host, config.port);

    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("{} Cannot bind to {}: {}", "✗".red().bold(), addr, e);
        std::process::exit(1);
    });

    let pages  = Arc::new(RwLock::new(pages));
    let reload = Arc::new(AtomicBool::new(false));
    let pool   = ThreadPool::new(config.threads);

    // Start file watcher (hot reload)
    if config.hot_reload {
        start_watcher(
            config.project.clone(),
            Arc::clone(&pages),
            Arc::clone(&reload),
            rebuild_fn,
        );
    }

    // Open browser
    let url = format!("http://{}:{}", config.host, config.port);
    let _ = open::that(&url);

    // Accept connections
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s)  => s,
            Err(_) => continue,
        };

        let pages      = Arc::clone(&pages);
        let reload     = Arc::clone(&reload);
        let project    = config.project.clone();
        let hot_reload = config.hot_reload;

        pool.execute(move || {
            handle_connection(stream, pages, reload, project, hot_reload);
        });
    }
}
