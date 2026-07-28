/// BlissLang PWA & Mobile Support — Part 19 of the spec
///
/// Parses the `pwa:` block of `bliss.config` and generates, at build time:
///   - `manifest.json`   — the Web App Manifest
///   - `sw.js`           — a service worker implementing the configured cache strategy
///   - `_bliss_pwa.js`   — client runtime: service-worker registration, push
///                          notification helpers (`PWA.pushSupported()`,
///                          `PWA.pushSubscribed()`, `PWA.requestPushPermission()`),
///                          and an install-prompt helper.
///
/// No external crates — hand-written parser and string templates, matching
/// the rest of BlissLang's zero-dependency philosophy. This module is
/// intentionally independent from `config::parse_config`'s generic
/// `ConfigValue::Table` parser, because the `icons:` field is a list of
/// tables (`- src: ... / sizes: ... / type: ...`), which the generic parser
/// does not yet support (see the note in config.rs's own test suite).

// ─── Data Model ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PwaIcon {
    pub src:     String,
    pub sizes:   String,
    pub type_:   String,
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CacheStrategy {
    NetworkFirst,
    CacheFirst,
    StaleWhileRevalidate,
}

impl CacheStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheStrategy::NetworkFirst => "network-first",
            CacheStrategy::CacheFirst => "cache-first",
            CacheStrategy::StaleWhileRevalidate => "stale-while-revalidate",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "cache-first" => CacheStrategy::CacheFirst,
            "stale-while-revalidate" => CacheStrategy::StaleWhileRevalidate,
            _ => CacheStrategy::NetworkFirst, // default, matches the spec's default
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Display {
    Standalone,
    MinimalUi,
    Fullscreen,
    Browser,
}

impl Display {
    fn parse(s: &str) -> Self {
        match s {
            "minimal-ui" => Display::MinimalUi,
            "fullscreen" => Display::Fullscreen,
            "browser"    => Display::Browser,
            _            => Display::Standalone,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Display::Standalone => "standalone",
            Display::MinimalUi  => "minimal-ui",
            Display::Fullscreen => "fullscreen",
            Display::Browser    => "browser",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Orientation {
    Portrait,
    Landscape,
    Any,
}

impl Orientation {
    fn parse(s: &str) -> Self {
        match s {
            "landscape" => Orientation::Landscape,
            "any"       => Orientation::Any,
            _           => Orientation::Portrait,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Orientation::Portrait  => "portrait",
            Orientation::Landscape => "landscape",
            Orientation::Any       => "any",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PwaConfig {
    pub enabled:            bool,
    pub name:                String,
    pub short_name:          String,
    pub description:         String,
    pub theme_color:         String,
    pub background:          String,
    pub display:             Display,
    pub orientation:         Orientation,
    pub start_url:           String,
    pub scope:                String,
    pub icons:               Vec<PwaIcon>,
    pub offline_page:        Option<String>,
    pub cache_strategy:      CacheStrategy,
    pub cache_pages:         Vec<String>,
    pub push_notifications:  bool,
}

impl Default for PwaConfig {
    fn default() -> Self {
        Self {
            enabled:           false,
            name:              String::new(),
            short_name:        String::new(),
            description:       String::new(),
            theme_color:       "#000000".to_string(),
            background:        "#FFFFFF".to_string(),
            display:           Display::Standalone,
            orientation:       Orientation::Portrait,
            start_url:         "/".to_string(),
            scope:             "/".to_string(),
            icons:             Vec::new(),
            offline_page:      None,
            cache_strategy:    CacheStrategy::NetworkFirst,
            cache_pages:       Vec::new(),
            push_notifications: false,
        }
    }
}

// ─── Parsing ────────────────────────────────────────────────────────────────

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes_ok = (s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''));
        if bytes_ok {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn parse_inline_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        inner
            .split(',')
            .map(|p| unquote(p.trim()))
            .filter(|p| !p.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

/// Find the `pwa:` block in raw `bliss.config` text and parse it.
/// Returns `None` if the project has no `pwa:` key at all.
pub fn parse_pwa_block(content: &str) -> Option<PwaConfig> {
    let lines: Vec<&str> = content.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let this_indent = indent_of(line);

        // Only match a top-level (unindented) `pwa:` key — matches how
        // `output:`, `i18n:`, etc. are declared in bliss.config.
        if this_indent == 0 && (trimmed == "pwa:" || trimmed.starts_with("pwa:")) {
            let mut body: Vec<&str> = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j];
                if l.trim().is_empty() {
                    body.push(l);
                    j += 1;
                    continue;
                }
                if indent_of(l) > this_indent {
                    body.push(l);
                    j += 1;
                } else {
                    break;
                }
            }
            return Some(parse_pwa_body(&body));
        }
        i += 1;
    }
    None
}

fn parse_pwa_body(body: &[&str]) -> PwaConfig {
    let mut cfg = PwaConfig::default();

    let base_indent = match body.iter().find(|l| !l.trim().is_empty()) {
        Some(l) => indent_of(l),
        None => return cfg,
    };

    let mut i = 0usize;
    while i < body.len() {
        let line = body[i];
        if line.trim().is_empty() || indent_of(line) != base_indent {
            i += 1;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        if let Some(colon) = trimmed.find(':') {
            let key = trimmed[..colon].trim();
            let rest = trimmed[colon + 1..].trim();

            match key {
                "enabled"            => cfg.enabled = rest == "true",
                "name"               => cfg.name = unquote(rest),
                "short_name"         => cfg.short_name = unquote(rest),
                "description"        => cfg.description = unquote(rest),
                "theme_color"        => cfg.theme_color = unquote(rest),
                "background"         => cfg.background = unquote(rest),
                "display"            => cfg.display = Display::parse(&unquote(rest)),
                "orientation"        => cfg.orientation = Orientation::parse(&unquote(rest)),
                "start_url"          => cfg.start_url = unquote(rest),
                "scope"              => cfg.scope = unquote(rest),
                "offline_page"       => cfg.offline_page = Some(unquote(rest)),
                "cache_strategy"     => cfg.cache_strategy = CacheStrategy::parse(&unquote(rest)),
                "push_notifications" => cfg.push_notifications = rest == "true",
                "cache_pages"        => cfg.cache_pages = parse_inline_array(rest),
                "icons" => {
                    let (icons, consumed) = parse_icon_list(&body[i + 1..], base_indent);
                    cfg.icons = icons;
                    i += consumed;
                }
                _ => {}
            }
        }
        i += 1;
    }

    cfg
}

/// Parses a `- src: ... / sizes: ... / type: ...` style list directly
/// beneath an `icons:` key. Returns the parsed icons plus how many lines
/// (relative to the start of `lines`) were consumed by the list, so the
/// caller can skip past them.
fn parse_icon_list(lines: &[&str], parent_indent: usize) -> (Vec<PwaIcon>, usize) {
    let mut icons: Vec<PwaIcon> = Vec::new();

    let list_indent = match lines.iter().find(|l| !l.trim().is_empty()) {
        Some(l) if indent_of(l) > parent_indent => indent_of(l),
        _ => return (icons, 0),
    };

    let mut current: Option<PwaIcon> = None;
    let mut consumed = 0usize;
    let mut idx = 0usize;

    while idx < lines.len() {
        let line = lines[idx];
        if line.trim().is_empty() {
            idx += 1;
            consumed += 1;
            continue;
        }
        let ind = indent_of(line);
        if ind < list_indent {
            break; // dedented out of the icons list
        }

        let trimmed = line.trim_start();
        if ind == list_indent && trimmed.starts_with("- ") {
            if let Some(icon) = current.take() {
                icons.push(icon);
            }
            let mut icon = PwaIcon { src: String::new(), sizes: String::new(), type_: String::new(), purpose: None };
            apply_icon_field(&mut icon, &trimmed[2..]);
            current = Some(icon);
        } else if let Some(icon) = current.as_mut() {
            apply_icon_field(icon, trimmed);
        }

        idx += 1;
        consumed += 1;
    }

    if let Some(icon) = current.take() {
        icons.push(icon);
    }

    (icons, consumed)
}

fn apply_icon_field(icon: &mut PwaIcon, field_line: &str) {
    if let Some(colon) = field_line.find(':') {
        let key = field_line[..colon].trim();
        let val = unquote(field_line[colon + 1..].trim());
        match key {
            "src"     => icon.src = val,
            "sizes"   => icon.sizes = val,
            "type"    => icon.type_ = val,
            "purpose" => icon.purpose = Some(val),
            _ => {}
        }
    }
}

// ─── manifest.json Generation ───────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Generate the contents of `manifest.json` from a `PwaConfig`.
pub fn generate_manifest(cfg: &PwaConfig) -> String {
    let short_name = if cfg.short_name.is_empty() { &cfg.name } else { &cfg.short_name };

    let mut icons_json = String::new();
    for (idx, icon) in cfg.icons.iter().enumerate() {
        if idx > 0 {
            icons_json.push_str(",\n");
        }
        icons_json.push_str("    {\n");
        icons_json.push_str(&format!("      \"src\": \"{}\",\n", json_escape(&icon.src)));
        icons_json.push_str(&format!("      \"sizes\": \"{}\",\n", json_escape(&icon.sizes)));
        icons_json.push_str(&format!("      \"type\": \"{}\"", json_escape(&icon.type_)));
        if let Some(p) = &icon.purpose {
            icons_json.push_str(&format!(",\n      \"purpose\": \"{}\"", json_escape(p)));
        }
        icons_json.push_str("\n    }");
    }

    format!(
        "{{\n  \"name\": \"{name}\",\n  \"short_name\": \"{short_name}\",\n  \"description\": \"{description}\",\n  \"start_url\": \"{start_url}\",\n  \"scope\": \"{scope}\",\n  \"display\": \"{display}\",\n  \"orientation\": \"{orientation}\",\n  \"background_color\": \"{background}\",\n  \"theme_color\": \"{theme_color}\",\n  \"icons\": [\n{icons}\n  ]\n}}\n",
        name = json_escape(&cfg.name),
        short_name = json_escape(short_name),
        description = json_escape(&cfg.description),
        start_url = json_escape(&cfg.start_url),
        scope = json_escape(&cfg.scope),
        display = cfg.display.as_str(),
        orientation = cfg.orientation.as_str(),
        background = json_escape(&cfg.background),
        theme_color = json_escape(&cfg.theme_color),
        icons = icons_json,
    )
}

// ─── Service Worker Generation ──────────────────────────────────────────────

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
}

/// Generate the contents of the service worker (commonly written to `sw.js`).
pub fn generate_service_worker(cfg: &PwaConfig, project_name: &str) -> String {
    let cache_name = format!("bliss-{}-v1", slugify(project_name));

    let mut precache = cfg.cache_pages.clone();
    if let Some(offline) = &cfg.offline_page {
        if !precache.iter().any(|p| p == offline) {
            precache.push(offline.clone());
        }
    }
    let precache_js = precache
        .iter()
        .map(|p| format!("  '{}'", p.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(",\n");

    let offline_fallback = match &cfg.offline_page {
        Some(p) => format!("caches.match('{}')", p.replace('\'', "\\'")),
        None => "caches.match(request)".to_string(),
    };

    let fetch_body = match cfg.cache_strategy {
        CacheStrategy::CacheFirst => format!(
"  event.respondWith(
    caches.match(request).then(function(cached) {{
      if (cached) return cached;
      return fetch(request).then(function(response) {{
        var copy = response.clone();
        caches.open(CACHE_NAME).then(function(cache) {{ cache.put(request, copy); }});
        return response;
      }}).catch(function() {{ return {fallback}; }});
    }})
  );", fallback = offline_fallback),

        CacheStrategy::StaleWhileRevalidate => format!(
"  event.respondWith(
    caches.match(request).then(function(cached) {{
      var fetchPromise = fetch(request).then(function(response) {{
        var copy = response.clone();
        caches.open(CACHE_NAME).then(function(cache) {{ cache.put(request, copy); }});
        return response;
      }}).catch(function() {{ return cached || {fallback}; }});
      return cached || fetchPromise;
    }})
  );", fallback = offline_fallback),

        CacheStrategy::NetworkFirst => format!(
"  event.respondWith(
    fetch(request).then(function(response) {{
      var copy = response.clone();
      caches.open(CACHE_NAME).then(function(cache) {{ cache.put(request, copy); }});
      return response;
    }}).catch(function() {{
      return caches.match(request).then(function(cached) {{
        return cached || {fallback};
      }});
    }})
  );", fallback = offline_fallback),
    };

    let push_handlers = if cfg.push_notifications {
        "\nself.addEventListener('push', function(event) {\n  var data = {};\n  try { data = event.data ? event.data.json() : {}; } catch (e) {}\n  var title = data.title || 'Notification';\n  var options = {\n    body: data.body || '',\n    icon: data.icon,\n    badge: data.badge,\n    data: data.url ? { url: data.url } : {}\n  };\n  event.waitUntil(self.registration.showNotification(title, options));\n});\n\nself.addEventListener('notificationclick', function(event) {\n  event.notification.close();\n  var url = (event.notification.data && event.notification.data.url) || '/';\n  event.waitUntil(clients.openWindow(url));\n});\n"
    } else {
        ""
    };

    format!(
"// Auto-generated by BlissLang — do not edit by hand.
// Cache strategy: {strategy}
var CACHE_NAME = '{cache_name}';
var PRECACHE_URLS = [
{precache}
];

self.addEventListener('install', function(event) {{
  self.skipWaiting();
  event.waitUntil(
    caches.open(CACHE_NAME).then(function(cache) {{
      return cache.addAll(PRECACHE_URLS);
    }})
  );
}});

self.addEventListener('activate', function(event) {{
  event.waitUntil(
    caches.keys().then(function(keys) {{
      return Promise.all(keys.filter(function(k) {{ return k !== CACHE_NAME; }})
        .map(function(k) {{ return caches.delete(k); }}));
    }}).then(function() {{ return self.clients.claim(); }})
  );
}});

self.addEventListener('fetch', function(event) {{
  var request = event.request;
  if (request.method !== 'GET') return;
{fetch_body}
}});
{push_handlers}",
        strategy = cfg.cache_strategy.as_str(),
        cache_name = cache_name,
        precache = precache_js,
        fetch_body = fetch_body,
        push_handlers = push_handlers,
    )
}

// ─── Client Runtime (registration + push helpers + install prompt) ────────

/// Generate the small client-side runtime that registers the service
/// worker and exposes `PWA.pushSupported()`, `PWA.pushSubscribed()`, and
/// `PWA.requestPushPermission()` on `window.__bliss.pwa` — matching the
/// spec's `PWA.*` calls used inside `OnMount`/`Async` blocks.
pub fn generate_client_runtime(cfg: &PwaConfig) -> String {
    let push_supported = if cfg.push_notifications { "true" } else { "false" };

    format!(
"// Auto-generated by BlissLang — PWA client runtime.
(function() {{
  'use strict';
  window.__bliss = window.__bliss || {{}};

  var pwa = {{
    pushSupported: function() {{
      return {push_supported} && 'serviceWorker' in navigator && 'PushManager' in window;
    }},
    pushSubscribed: function() {{
      if (!pwa.pushSupported()) return Promise.resolve(false);
      return navigator.serviceWorker.ready.then(function(reg) {{
        return reg.pushManager.getSubscription().then(function(sub) {{ return !!sub; }});
      }});
    }},
    requestPushPermission: function() {{
      if (!pwa.pushSupported()) return Promise.reject(new Error('Push not supported'));
      return Notification.requestPermission().then(function(permission) {{
        if (permission !== 'granted') throw new Error('Permission denied');
        return navigator.serviceWorker.ready;
      }}).then(function(reg) {{
        return reg.pushManager.subscribe({{ userVisibleOnly: true }});
      }});
    }},
    deferredInstallPrompt: null,
    canInstall: function() {{ return !!pwa.deferredInstallPrompt; }},
    promptInstall: function() {{
      if (!pwa.deferredInstallPrompt) return Promise.resolve('unavailable');
      var p = pwa.deferredInstallPrompt;
      pwa.deferredInstallPrompt = null;
      p.prompt();
      return p.userChoice.then(function(choice) {{ return choice.outcome; }});
    }}
  }};

  window.__bliss.pwa = pwa;
  window.PWA = pwa;

  window.addEventListener('beforeinstallprompt', function(event) {{
    event.preventDefault();
    pwa.deferredInstallPrompt = event;
  }});

  if ('serviceWorker' in navigator) {{
    window.addEventListener('load', function() {{
      navigator.serviceWorker.register('/sw.js').catch(function(err) {{
        console.warn('[BlissLang] Service worker registration failed:', err);
      }});
    }});
  }}
}})();
",
        push_supported = push_supported,
    )
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
project: "MyApp"
output: "static"

pwa:
    enabled: true
    name: "MyApp"
    short_name: "MyApp"
    description: "Enterprise dashboard"
    theme_color: "#1A1A2E"
    background: "#FFFFFF"
    display: "standalone"
    orientation: "portrait"
    start_url: "/"
    scope: "/"
    icons:
        - src: "@Images/icon-192.png"
          sizes: "192x192"
          type: "image/png"
        - src: "@Images/icon-512.png"
          sizes: "512x512"
          type: "image/png"
        - src: "@Images/icon-maskable.png"
          sizes: "512x512"
          type: "image/png"
          purpose: "maskable"
    offline_page: "/offline"
    cache_strategy: "network-first"
    cache_pages: ["/", "/dashboard", "/products"]
    push_notifications: true
"##;

    #[test]
    fn test_no_pwa_block_returns_none() {
        assert_eq!(parse_pwa_block("project: \"MyApp\"\noutput: \"static\""), None);
    }

    #[test]
    fn test_parse_pwa_scalars() {
        let cfg = parse_pwa_block(SAMPLE).expect("pwa block should parse");
        assert!(cfg.enabled);
        assert_eq!(cfg.name, "MyApp");
        assert_eq!(cfg.theme_color, "#1A1A2E");
        assert_eq!(cfg.display, Display::Standalone);
        assert_eq!(cfg.orientation, Orientation::Portrait);
        assert_eq!(cfg.offline_page.as_deref(), Some("/offline"));
        assert_eq!(cfg.cache_strategy, CacheStrategy::NetworkFirst);
        assert!(cfg.push_notifications);
    }

    #[test]
    fn test_parse_pwa_cache_pages() {
        let cfg = parse_pwa_block(SAMPLE).unwrap();
        assert_eq!(cfg.cache_pages, vec!["/", "/dashboard", "/products"]);
    }

    #[test]
    fn test_parse_pwa_icons() {
        let cfg = parse_pwa_block(SAMPLE).unwrap();
        assert_eq!(cfg.icons.len(), 3);
        assert_eq!(cfg.icons[0].src, "@Images/icon-192.png");
        assert_eq!(cfg.icons[0].sizes, "192x192");
        assert_eq!(cfg.icons[0].purpose, None);
        assert_eq!(cfg.icons[2].purpose.as_deref(), Some("maskable"));
    }

    #[test]
    fn test_generate_manifest_contains_icons_and_theme() {
        let cfg = parse_pwa_block(SAMPLE).unwrap();
        let manifest = generate_manifest(&cfg);
        assert!(manifest.contains("\"theme_color\": \"#1A1A2E\""));
        assert!(manifest.contains("\"display\": \"standalone\""));
        assert!(manifest.contains("icon-maskable.png"));
        assert!(manifest.contains("\"purpose\": \"maskable\""));
    }

    #[test]
    fn test_generate_service_worker_network_first() {
        let cfg = parse_pwa_block(SAMPLE).unwrap();
        let sw = generate_service_worker(&cfg, "MyApp");
        assert!(sw.contains("network-first"));
        assert!(sw.contains("/offline"));
        assert!(sw.contains("'/dashboard'"));
        assert!(sw.contains("addEventListener('push'"));
    }

    #[test]
    fn test_generate_service_worker_cache_first() {
        let mut cfg = PwaConfig::default();
        cfg.cache_strategy = CacheStrategy::CacheFirst;
        let sw = generate_service_worker(&cfg, "MyApp");
        assert!(sw.contains("caches.match(request).then"));
        assert!(!sw.contains("addEventListener('push'")); // push disabled by default
    }

    #[test]
    fn test_client_runtime_registers_service_worker() {
        let cfg = PwaConfig::default();
        let js = generate_client_runtime(&cfg);
        assert!(js.contains("serviceWorker.register('/sw.js')"));
        assert!(js.contains("pushSupported"));
    }
}
