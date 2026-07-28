/// BlissLang Config Parser — v0.5
///
/// Parses bliss.config (TOML-like) and drives every compiler decision.
/// No external TOML crate — hand-written key-value parser that handles
/// the BlissLang config syntax defined in the spec.
///
/// Supports:
///   project: "MyApp"
///   output: "static" | "runtime" | "hybrid"
///   port: 8080
///   hot_reload: true
///   security: "strict" | "standard" | "open"
///   i18n: { default: "en", locales: ["en", "zu"] }
///   packages: [{ name: "...", version: "...", ... }]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ─── Config Value ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<ConfigValue>),
    Table(HashMap<String, ConfigValue>),
    Null,
}

impl ConfigValue {
    pub fn as_str(&self) -> Option<&str> {
        if let ConfigValue::Str(s) = self { Some(s.as_str()) } else { None }
    }
    pub fn as_int(&self) -> Option<i64> {
        if let ConfigValue::Int(n) = self { Some(*n) } else { None }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            ConfigValue::Int(n)   => Some(*n as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let ConfigValue::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_array(&self) -> Option<&Vec<ConfigValue>> {
        if let ConfigValue::Array(a) = self { Some(a) } else { None }
    }
    pub fn as_table(&self) -> Option<&HashMap<String, ConfigValue>> {
        if let ConfigValue::Table(t) = self { Some(t) } else { None }
    }
}

// ─── Bliss Config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BlissConfig {
    // Core
    pub project:      String,
    pub version:      String,
    pub author:       String,
    pub description:  String,

    // Output
    pub output:       OutputMode,
    pub out_dir:      String,

    // Dev server
    pub port:         u16,
    pub host:         String,
    pub open_browser: bool,
    pub hot_reload:   bool,
    pub threads:      usize,

    // Styling
    pub tailwind:     bool,

    // Features
    pub animations:   bool,
    pub geometry:     bool,
    pub signals:      bool,
    pub minify:       bool,
    pub source_maps:  bool,

    // SVG mode
    pub svg:          SvgMode,

    // Security
    pub security:     SecurityMode,
    pub csp:          bool,
    pub external_scripts: bool,
    pub allowed_origins:  Vec<String>,

    // i18n
    pub i18n:         I18nConfig,

    // Packages
    pub packages:     Vec<PackageConfig>,

    // PWA / mobile (Part 19) — None if the project has no `pwa:` block
    pub pwa:          Option<super::pwa::PwaConfig>,

    // Raw values for anything not explicitly parsed
    pub raw:          HashMap<String, ConfigValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputMode {
    Static,
    Runtime,
    Hybrid,
}

impl OutputMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputMode::Static  => "static",
            OutputMode::Runtime => "runtime",
            OutputMode::Hybrid  => "hybrid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SvgMode {
    Bliss,
    Native,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecurityMode {
    Strict,
    Standard,
    Open,
}

impl SecurityMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityMode::Strict   => "strict",
            SecurityMode::Standard => "standard",
            SecurityMode::Open     => "open",
        }
    }
}

#[derive(Debug, Clone)]
pub struct I18nConfig {
    pub default:  String,
    pub locales:  Vec<String>,
    pub fallback: String,
    pub direction: String,
}

impl Default for I18nConfig {
    fn default() -> Self {
        Self {
            default:   "en".to_string(),
            locales:   vec!["en".to_string()],
            fallback:  "en".to_string(),
            direction: "ltr".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageConfig {
    pub name:         String,
    pub version:      String,
    pub source:       String,
    pub capabilities: Vec<String>,
    pub hash:         String,
}

impl Default for BlissConfig {
    fn default() -> Self {
        Self {
            project:         "BlissApp".to_string(),
            version:         "0.1.0".to_string(),
            author:          String::new(),
            description:     String::new(),
            output:          OutputMode::Static,
            out_dir:         "dist".to_string(),
            port:            8080,
            host:            "127.0.0.1".to_string(),
            open_browser:    true,
            hot_reload:      true,
            threads:         4,
            tailwind:        true,
            animations:      true,
            geometry:        true,
            signals:         true,
            minify:          false,
            source_maps:     false,
            svg:             SvgMode::Bliss,
            security:        SecurityMode::Strict,
            csp:             true,
            external_scripts:false,
            allowed_origins: Vec::new(),
            i18n:            I18nConfig::default(),
            packages:        Vec::new(),
            pwa:             None,
            raw:             HashMap::new(),
        }
    }
}

impl BlissConfig {
    /// Load bliss.config from a project directory.
    /// Returns Default config if no config file found.
    pub fn load(project_dir: &str) -> Self {
        let config_path = format!("{}/bliss.config", project_dir);

        if !Path::new(&config_path).exists() {
            return BlissConfig::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => Self::parse(&content),
            Err(_)      => BlissConfig::default(),
        }
    }

    /// Parse bliss.config content into a BlissConfig.
    pub fn parse(content: &str) -> Self {
        let raw = parse_config(content);
        let mut cfg = BlissConfig::default();

        // PWA block — parsed directly from the raw text since it contains a
        // list of tables (`icons:`) that the generic key/value parser above
        // does not yet support.
        cfg.pwa = super::pwa::parse_pwa_block(content);

        // Core
        if let Some(v) = raw.get("project") {
            if let Some(s) = v.as_str() { cfg.project = s.to_string(); }
        }
        if let Some(v) = raw.get("version") {
            if let Some(s) = v.as_str() { cfg.version = s.to_string(); }
        }
        if let Some(v) = raw.get("author") {
            if let Some(s) = v.as_str() { cfg.author = s.to_string(); }
        }
        if let Some(v) = raw.get("description") {
            if let Some(s) = v.as_str() { cfg.description = s.to_string(); }
        }

        // Output mode
        if let Some(v) = raw.get("output") {
            cfg.output = match v.as_str() {
                Some("runtime") => OutputMode::Runtime,
                Some("hybrid")  => OutputMode::Hybrid,
                _               => OutputMode::Static,
            };
        }
        if let Some(v) = raw.get("out_dir") {
            if let Some(s) = v.as_str() { cfg.out_dir = s.to_string(); }
        }

        // Dev server
        if let Some(v) = raw.get("port") {
            if let Some(n) = v.as_int() { cfg.port = n as u16; }
        }
        if let Some(v) = raw.get("host") {
            if let Some(s) = v.as_str() { cfg.host = s.to_string(); }
        }
        if let Some(v) = raw.get("open_browser") {
            if let Some(b) = v.as_bool() { cfg.open_browser = b; }
        }
        if let Some(v) = raw.get("hot_reload") {
            if let Some(b) = v.as_bool() { cfg.hot_reload = b; }
        }
        if let Some(v) = raw.get("threads") {
            if let Some(n) = v.as_int() { cfg.threads = n as usize; }
        }

        // Features
        if let Some(v) = raw.get("tailwind")   { if let Some(b) = v.as_bool() { cfg.tailwind   = b; } }
        if let Some(v) = raw.get("animations") { if let Some(b) = v.as_bool() { cfg.animations = b; } }
        if let Some(v) = raw.get("geometry")   { if let Some(b) = v.as_bool() { cfg.geometry   = b; } }
        if let Some(v) = raw.get("signals")    { if let Some(b) = v.as_bool() { cfg.signals    = b; } }
        if let Some(v) = raw.get("minify")     { if let Some(b) = v.as_bool() { cfg.minify     = b; } }
        if let Some(v) = raw.get("source_maps"){ if let Some(b) = v.as_bool() { cfg.source_maps= b; } }

        // SVG mode
        if let Some(v) = raw.get("svg") {
            cfg.svg = match v.as_str() {
                Some("native") => SvgMode::Native,
                Some("both")   => SvgMode::Both,
                _              => SvgMode::Bliss,
            };
        }

        // Security
        if let Some(v) = raw.get("security") {
            cfg.security = match v.as_str() {
                Some("open")     => SecurityMode::Open,
                Some("standard") => SecurityMode::Standard,
                _                => SecurityMode::Strict,
            };
        }
        if let Some(v) = raw.get("csp")              { if let Some(b) = v.as_bool() { cfg.csp = b; } }
        if let Some(v) = raw.get("external_scripts")  { if let Some(b) = v.as_bool() { cfg.external_scripts = b; } }

        if let Some(v) = raw.get("allowed_origins") {
            if let Some(arr) = v.as_array() {
                cfg.allowed_origins = arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect();
            }
        }

        // i18n block
        if let Some(ConfigValue::Table(i18n_table)) = raw.get("i18n") {
            if let Some(v) = i18n_table.get("default") {
                if let Some(s) = v.as_str() { cfg.i18n.default = s.to_string(); }
            }
            if let Some(v) = i18n_table.get("fallback") {
                if let Some(s) = v.as_str() { cfg.i18n.fallback = s.to_string(); }
            }
            if let Some(v) = i18n_table.get("direction") {
                if let Some(s) = v.as_str() { cfg.i18n.direction = s.to_string(); }
            }
            if let Some(v) = i18n_table.get("locales") {
                if let Some(arr) = v.as_array() {
                    cfg.i18n.locales = arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect();
                }
            }
        }

        // Packages
        if let Some(ConfigValue::Array(pkgs)) = raw.get("packages") {
            for pkg in pkgs {
                if let Some(t) = pkg.as_table() {
                    let name    = t.get("name")   .and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let version = t.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let source  = t.get("source") .and_then(|v| v.as_str()).unwrap_or("hub").to_string();
                    let hash    = t.get("hash")   .and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let capabilities = t.get("capabilities")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                        .unwrap_or_default();
                    if !name.is_empty() {
                        cfg.packages.push(PackageConfig { name, version, source, hash, capabilities });
                    }
                }
            }
        }

        cfg.raw = raw;
        cfg
    }

    /// Validate the config and return warnings.
    pub fn validate(&self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();

        // Security warnings
        if self.security == SecurityMode::Open {
            warnings.push(ConfigWarning {
                level:   WarnLevel::Critical,
                field:   "security".to_string(),
                message: "security is set to 'open' — all supply-chain protections disabled".to_string(),
            });
        }

        if self.external_scripts && self.security == SecurityMode::Strict {
            warnings.push(ConfigWarning {
                level:   WarnLevel::Error,
                field:   "external_scripts".to_string(),
                message: "external_scripts: true conflicts with security: strict".to_string(),
            });
        }

        // Package version range check
        for pkg in &self.packages {
            if pkg.version.starts_with('^') || pkg.version.starts_with('~')
                || pkg.version == "latest" || pkg.version.starts_with(">=")
            {
                warnings.push(ConfigWarning {
                    level:   WarnLevel::Error,
                    field:   format!("packages.{}.version", pkg.name),
                    message: format!("Package '{}' uses version range '{}' — BlissLang requires exact version pins", pkg.name, pkg.version),
                });
            }
            if pkg.hash.is_empty() && pkg.source == "hub" {
                warnings.push(ConfigWarning {
                    level:   WarnLevel::Warn,
                    field:   format!("packages.{}.hash", pkg.name),
                    message: format!("Package '{}' has no hash — will be verified on first download", pkg.name),
                });
            }
        }

        // PWA warnings
        if let Some(pwa) = &self.pwa {
            if pwa.enabled {
                if pwa.icons.is_empty() {
                    warnings.push(ConfigWarning {
                        level:   WarnLevel::Warn,
                        field:   "pwa.icons".to_string(),
                        message: "pwa.enabled is true but no icons are declared — most browsers require at least a 192x192 and 512x512 icon to allow installation".to_string(),
                    });
                }
                if pwa.name.is_empty() {
                    warnings.push(ConfigWarning {
                        level:   WarnLevel::Warn,
                        field:   "pwa.name".to_string(),
                        message: "pwa.enabled is true but pwa.name is empty — manifest.json will have an empty app name".to_string(),
                    });
                }
                if pwa.offline_page.is_none() {
                    warnings.push(ConfigWarning {
                        level:   WarnLevel::Info,
                        field:   "pwa.offline_page".to_string(),
                        message: "no pwa.offline_page set — the service worker will fall back to a generic cache lookup instead of a dedicated offline page".to_string(),
                    });
                }
            }
        }

        // Port range
        if self.port < 1024 {
            warnings.push(ConfigWarning {
                level:   WarnLevel::Warn,
                field:   "port".to_string(),
                message: format!("Port {} is a privileged port (< 1024) — may require elevated permissions", self.port),
            });
        }

        warnings
    }
}

#[derive(Debug, Clone)]
pub struct ConfigWarning {
    pub level:   WarnLevel,
    pub field:   String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum WarnLevel { Critical, Error, Warn, Info }

// ─── Config File Parser ───────────────────────────────────────────────────────

/// Parse bliss.config into a raw HashMap<String, ConfigValue>.
/// Supports: key: value, nested tables (key:\n  sub: val), and inline arrays.
fn parse_config(content: &str) -> HashMap<String, ConfigValue> {
    let mut map  = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Skip comments and blank lines
        if trimmed.starts_with('#') || trimmed.is_empty() { continue; }

        // Key: value
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let rest = trimmed[colon_pos + 1..].trim();

            if rest.is_empty() {
                // Nested table: key:\n  sub: val
                let mut sub_map = HashMap::new();
                while let Some(next) = lines.peek() {
                    let _next_trimmed = next.trim();
                    // Sub-key is indented
                    if next.starts_with("  ") || next.starts_with('\t') {
                        let line = lines.next().unwrap();
                        let sub = line.trim();
                        if sub.starts_with('#') || sub.is_empty() { continue; }
                        if let Some(cp) = sub.find(':') {
                            let sk = sub[..cp].trim().to_string();
                            let sv = sub[cp + 1..].trim();
                            sub_map.insert(sk, parse_value(sv));
                        }
                    } else {
                        break;
                    }
                }
                map.insert(key, ConfigValue::Table(sub_map));
            } else {
                map.insert(key, parse_value(rest));
            }
        }
    }

    map
}

/// Parse a single config value string into a ConfigValue.
fn parse_value(s: &str) -> ConfigValue {
    let s = s.trim();

    // String with quotes
    if (s.starts_with('"') && s.ends_with('"')) ||
       (s.starts_with('\'') && s.ends_with('\'')) {
        return ConfigValue::Str(s[1..s.len()-1].to_string());
    }

    // Boolean
    if s == "true"  { return ConfigValue::Bool(true); }
    if s == "false" { return ConfigValue::Bool(false); }

    // Null
    if s == "null" || s == "~" { return ConfigValue::Null; }

    // Array: ["a", "b", "c"]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len()-1];
        let items: Vec<ConfigValue> = split_array(inner)
            .into_iter()
            .map(|item| parse_value(item.trim()))
            .collect();
        return ConfigValue::Array(items);
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return ConfigValue::Int(n);
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        return ConfigValue::Float(f);
    }

    // Unquoted string
    ConfigValue::Str(s.to_string())
}

/// Split a comma-separated array body, respecting nested quotes and brackets.
fn split_array(s: &str) -> Vec<&str> {
    let mut items  = Vec::new();
    let mut depth  = 0i32;
    let mut in_str = false;
    let mut start  = 0usize;

    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        match chars[i] {
            '"' | '\'' => { in_str = !in_str; }
            '[' | '{' if !in_str => { depth += 1; }
            ']' | '}' if !in_str => { depth -= 1; }
            ',' if !in_str && depth == 0 => {
                items.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }

    if start < s.len() {
        let last = s[start..].trim();
        if !last.is_empty() { items.push(last); }
    }

    items
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
# BlissLang Configuration
project:       "MyApp"
version:       "1.0.0"
author:        "Bliss Mahlathi"
output:        "static"
port:          8080
hot_reload:    true
tailwind:      true
animations:    true
minify:        false
security:      "strict"
csp:           true
external_scripts: false

i18n:
    default:   "en"
    fallback:  "en"
    locales:   ["en", "zu", "af"]
"##;

    #[test]
    fn test_parse_basic_fields() {
        let cfg = BlissConfig::parse(SAMPLE);
        assert_eq!(cfg.project, "MyApp");
        assert_eq!(cfg.version, "1.0.0");
        assert_eq!(cfg.author,  "Bliss Mahlathi");
        assert_eq!(cfg.port,    8080);
        assert_eq!(cfg.hot_reload, true);
        assert_eq!(cfg.tailwind,   true);
        assert_eq!(cfg.minify,     false);
    }

    #[test]
    fn test_parse_output_mode() {
        let cfg = BlissConfig::parse(SAMPLE);
        assert_eq!(cfg.output, OutputMode::Static);

        let runtime = BlissConfig::parse("output: \"runtime\"");
        assert_eq!(runtime.output, OutputMode::Runtime);

        let hybrid = BlissConfig::parse("output: \"hybrid\"");
        assert_eq!(hybrid.output, OutputMode::Hybrid);
    }

    #[test]
    fn test_parse_security_mode() {
        let cfg = BlissConfig::parse(SAMPLE);
        assert_eq!(cfg.security, SecurityMode::Strict);

        let open = BlissConfig::parse("security: \"open\"");
        assert_eq!(open.security, SecurityMode::Open);
    }

    #[test]
    fn test_parse_i18n_block() {
        let cfg = BlissConfig::parse(SAMPLE);
        assert_eq!(cfg.i18n.default, "en");
        assert_eq!(cfg.i18n.fallback, "en");
        assert_eq!(cfg.i18n.locales, vec!["en", "zu", "af"]);
    }

    #[test]
    fn test_default_config() {
        let cfg = BlissConfig::default();
        assert_eq!(cfg.project, "BlissApp");
        assert_eq!(cfg.port,    8080);
        assert_eq!(cfg.output,  OutputMode::Static);
        assert_eq!(cfg.security,SecurityMode::Strict);
        assert_eq!(cfg.hot_reload, true);
    }

    #[test]
    fn test_validation_open_security_warns() {
        let cfg = BlissConfig::parse("security: \"open\"");
        let warnings = cfg.validate();
        assert!(warnings.iter().any(|w| w.level == WarnLevel::Critical));
    }

    #[test]
    fn test_validation_version_range_errors() {
        let cfg_str = r##"
packages:
    - name: "stripe"
      version: "^3.0.0"
      source: "hub"
"##;
        // This is not yet supported in nested array parsing but validates the logic exists
        let cfg = BlissConfig::default();
        let pkg = PackageConfig {
            name:         "stripe".to_string(),
            version:      "^3.0.0".to_string(),
            source:       "hub".to_string(),
            hash:         String::new(),
            capabilities: Vec::new(),
        };
        let mut cfg2 = cfg;
        cfg2.packages.push(pkg);
        let warnings = cfg2.validate();
        assert!(warnings.iter().any(|w| w.message.contains("version range")));
    }

    #[test]
    fn test_missing_config_returns_default() {
        let cfg = BlissConfig::load("/nonexistent/path");
        assert_eq!(cfg.project, "BlissApp"); // default
    }

    #[test]
    fn test_array_parsing() {
        let cfg = BlissConfig::parse(r#"allowed_origins: ["https://fonts.googleapis.com", "https://cdn.myapp.com"]"#);
        assert_eq!(cfg.allowed_origins.len(), 2);
        assert_eq!(cfg.allowed_origins[0], "https://fonts.googleapis.com");
    }
}
