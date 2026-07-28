/// BlissLang Compiler & Dev Server — v0.2
/// Author: Bliss Mahlathi — PulseBit, Nkowankowa, Limpopo
///
/// Zero axum. Zero tokio. Zero hyper.
/// HTTP server: std::net::TcpListener + thread pool.
/// Hot reload:  notify crate (file watcher) + SSE.

mod compiler;
mod runtime;
mod geo;

use compiler::lexer;
use compiler::parser;
use compiler::ast::*;
use compiler::renderer::{Renderer, RenderConfig};
use compiler::config::BlissConfig;
use compiler::typechecker;
use runtime::server::{ServerConfig, start};
use runtime::scaffold::{ScaffoldOptions, Template, scaffold};
// runtime::router used in build_pages_routed

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

// ─── CLI ─────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name    = "bliss",
    about   = "BlissLang v0.2 — Build websites section by section",
    version = "0.2.0",
    author  = "Bliss Mahlathi <bliss@pulsebit.dev>"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new BlissLang project
    New {
        /// Project name (creates a directory with this name)
        name: String,
        /// Project template: landing | dashboard | minimal
        #[arg(short, long, default_value = "landing")]
        template: String,
    },
    /// Start the development server with hot reload
    Dev {
        /// Project directory
        #[arg(default_value = ".")]
        project: String,
        /// Port to serve on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Disable hot reload
        #[arg(long)]
        no_reload: bool,
        /// Number of worker threads
        #[arg(short, long, default_value = "4")]
        threads: usize,
    },
    /// Build the project to an output directory
    Build {
        #[arg(default_value = ".")]
        project: String,
        #[arg(short, long, default_value = "dist")]
        out: String,
    },
    /// Check a .bliss file for syntax errors
    Check { file: String },
    /// Print the token stream for a file (debug)
    Tokens { file: String },
    /// Print the full AST for a file (debug)
    Ast { file: String },
    /// Build an optimised production release
    Release {
        #[arg(default_value = ".")]
        project: String,
        #[arg(short, long)]
        out: Option<String>,
    },
    /// Print project stats
    Info {
        #[arg(default_value = ".")]
        project: String,
    },
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    print_banner();
    let cli = Cli::parse();

    match cli.command {
        Command::New { name, template } => {
            cmd_new(&name, &template);
        }
        Command::Dev { project, port, no_reload, threads } => {
            cmd_dev(&project, port, !no_reload, threads);
        }
        Command::Build { project, out } => {
            cmd_build(&project, &out);
        }
        Command::Check { file }  => cmd_check(&file),
        Command::Tokens { file } => cmd_tokens(&file),
        Command::Ast { file }    => cmd_ast(&file),
        Command::Release { project, out } => {
            cmd_release(&project, out.as_deref());
        }
        Command::Info { project } => cmd_info(&project),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

fn cmd_new(name: &str, template_str: &str) {
    println!("{}", "✦ BlissLang New Project".bold().bright_cyan());
    println!();

    // Validate project name
    if name.contains('/') || name.contains('\\') || name.contains(' ') {
        eprintln!("{} Project name cannot contain spaces or slashes", "✗".red().bold());
        std::process::exit(1);
    }

    let opts = ScaffoldOptions {
        name:     name.to_string(),
        template: Template::from_str(template_str),
    };

    match scaffold(opts) {
        Ok(())  => {}
        Err(e)  => { eprintln!("{} {}", "✗".red().bold(), e); std::process::exit(1); }
    }
}

/// Write manifest.json, sw.js, and the client PWA runtime to `out_dir`
/// when the project's `bliss.config` has `pwa.enabled: true`. Silently
/// does nothing otherwise.
fn write_pwa_artifacts(cfg: &BlissConfig, out_dir: &str) {
    let pwa = match &cfg.pwa {
        Some(p) if p.enabled => p,
        _ => return,
    };

    let manifest = compiler::pwa::generate_manifest(pwa);
    fs::write(format!("{}/manifest.json", out_dir), &manifest)
        .expect("Cannot write manifest.json");
    println!("  {} {}/manifest.json", "✓".green(), out_dir);

    let sw = compiler::pwa::generate_service_worker(pwa, &cfg.project);
    fs::write(format!("{}/sw.js", out_dir), &sw)
        .expect("Cannot write sw.js");
    println!("  {} {}/sw.js ({})", "✓".green(), out_dir, pwa.cache_strategy.as_str());

    let client = compiler::pwa::generate_client_runtime(pwa);
    fs::write(format!("{}/_bliss_pwa.js", out_dir), &client)
        .expect("Cannot write _bliss_pwa.js");
    println!("  {} {}/_bliss_pwa.js", "✓".green(), out_dir);

    if pwa.icons.is_empty() {
        println!("  {} pwa.enabled is true but no icons are declared in bliss.config", "⚠".yellow());
    }
}

fn cmd_release(project: &str, out_override: Option<&str>) {
    println!("{}", "🚀 BlissLang Release Build".bold().bright_cyan());

    // Load config
    let cfg = BlissConfig::load(project);
    let out_dir = out_override.unwrap_or(&cfg.out_dir).to_string();

    println!("   {}: {}", "Project".dimmed(),  cfg.project.cyan());
    println!("   {}: {}", "Output".dimmed(),   out_dir.cyan());
    println!("   {}: {}", "Mode".dimmed(),     cfg.output.as_str().cyan());
    println!("   {}: {}", "Security".dimmed(), cfg.security.as_str().green());
    println!();

    // Config validation warnings
    for warn in cfg.validate() {
        match warn.level {
            compiler::config::WarnLevel::Critical | compiler::config::WarnLevel::Error => {
                eprintln!("  {} {} — {}", "✗".red(), warn.field.bold(), warn.message);
            }
            _ => {
                println!("  {} {} — {}", "⚠".yellow(), warn.field.dimmed(), warn.message);
            }
        }
    }

    // Load and typecheck
    let pf = match load_project(project) {
        Ok(p)  => p,
        Err(e) => { eprintln!("{} {}", "✗".red().bold(), e); std::process::exit(1); }
    };

    // Run type checker
    println!("{}", "Type checking...".bold());
    let all_files: Vec<BlissFile> = pf.sections.values().map(|s| BlissFile::Section(s.clone()))
        .chain(pf.divs.values().map(|d| BlissFile::Div(d.clone())))
        .chain(pf.pages.values().map(|p| BlissFile::Page(p.clone())))
        .chain(pf.states.values().map(|s| BlissFile::State(s.clone())))
        .collect();

    let section_names: Vec<String> = pf.sections.keys().cloned().collect();
    let div_names:     Vec<String> = pf.divs.keys().cloned().collect();
    let (type_errors, type_warnings) = typechecker::check_project(&all_files, &section_names, &div_names);

    for warn in &type_warnings {
        println!("  {} {}", "⚠".yellow(), warn);
    }
    for err in &type_errors {
        eprintln!("  {} {}", "✗".red(), err);
    }
    if !type_errors.is_empty() {
        eprintln!("\n{} {} type error(s) — fix before release", "✗".red().bold(), type_errors.len());
        std::process::exit(1);
    }
    if type_warnings.is_empty() && type_errors.is_empty() {
        println!("  {} All type checks passed", "✓".green());
    }
    println!();

    // Build pages with minification
    println!("{}", "Building...".bold());
    let pages = build_pages_minified(&pf, &cfg);
    fs::create_dir_all(&out_dir).expect("Cannot create output directory");

    let mut total_original = 0usize;
    let mut total_minified = 0usize;

    for (route, (html, minified)) in &pages {
        let path = if route == "/" {
            format!("{}/index.html", out_dir)
        } else {
            let clean = route.trim_start_matches('/');
            let dir   = format!("{}/{}", out_dir, clean);
            fs::create_dir_all(&dir).ok();
            format!("{}/index.html", dir)
        };
        fs::write(&path, minified).expect("Cannot write HTML");
        let savings = if html.len() > 0 { 100 - (minified.len() * 100 / html.len()) } else { 0 };
        println!("  {} {} ({} → {} bytes, {}% smaller)",
            "✓".green(), path,
            html.len(), minified.len(), savings);
        total_original += html.len();
        total_minified += minified.len();
    }

    // Write runtime JS (without hot reload)
    let runtime = runtime::server::runtime_js_static();
    let runtime_min = minify_js(&runtime);
    fs::write(format!("{}/_bliss_runtime.js", out_dir), &runtime_min)
        .expect("Cannot write runtime JS");
    println!("  {} {}/_bliss_runtime.js ({} bytes)", "✓".green(), out_dir, runtime_min.len());

    write_pwa_artifacts(&cfg, &out_dir);

    println!();
    let total_savings = if total_original > 0 {
        100 - (total_minified * 100 / total_original)
    } else { 0 };
    println!("{} {} pages → {}/ ({} → {} bytes, {}% smaller)",
        "✓".green().bold(),
        pages.len(), out_dir,
        total_original, total_minified, total_savings);
    println!();
    println!("{}", "Release ready. Deploy the dist/ folder.".green().bold());
}

fn cmd_dev(project: &str, port: u16, hot_reload: bool, threads: usize) {
    let cfg = BlissConfig::load(project);
    println!("{}", "🚀 BlissLang Dev Server v0.5".bold().bright_cyan());
    println!("   {}: {} ({})", "Project".dimmed(), cfg.project.cyan(), project);
    println!("   {}: http://localhost:{}", "URL".dimmed(), port);
    println!("   {}: {}", "Security".dimmed(), cfg.security.as_str().green());
    println!("   {}: {}", "Hot reload".dimmed(),
        if hot_reload { "enabled (SSE)".green().to_string() }
        else          { "disabled".dimmed().to_string() }
    );
    println!();

    for warn in cfg.validate() {
        println!("  {} {}", "⚠".yellow(), warn.message.dimmed());
    }

    let project_str = project.to_string();
    let pf = match load_project(&project_str) {
        Ok(p)  => p,
        Err(e) => { eprintln!("{} {}", "✗".red().bold(), e); std::process::exit(1); }
    };

    let pages = build_pages_routed(&pf);
    let (static_n, runtime_n) = {
        let r = runtime::router::build_router(&pages);
        r.stats()
    };
    println!("{} {} pages  {} sections  {} divs  {} states",
        "✓".green().bold(),
        pf.pages.len(), pf.sections.len(), pf.divs.len(), pf.states.len());
    if runtime_n > 0 {
        println!("  {} {} static  {} runtime", "→".cyan(), static_n, runtime_n);
    }
    println!();
    println!("{}", "Routes:".bold());
    for (route, (_, mode)) in &pages {
        let mode_str = match mode {
            compiler::ast::OutputMode::Runtime => " [runtime]".yellow().to_string(),
            compiler::ast::OutputMode::Hybrid  => " [hybrid]".cyan().to_string(),
            _                                   => String::new(),
        };
        println!("  {} http://localhost:{}{}{}", "→".cyan(), port, route, mode_str);
    }
    println!();

    let server_cfg = ServerConfig {
        port,
        host:       "127.0.0.1".into(),
        hot_reload,
        project:    project_str.clone(),
        threads,
    };

    let rebuild = move |proj: &str| -> HashMap<String, (String, compiler::ast::OutputMode)> {
        match load_project(proj) {
            Ok(pf) => build_pages_routed(&pf),
            Err(e) => {
                eprintln!("  {} Rebuild failed: {}", "✗".red(), e);
                HashMap::new()
            }
        }
    };

    println!("{}", format!("Listening on http://localhost:{}", port).green().bold());
    println!("{}", "Press Ctrl+C to stop".dimmed());
    println!();

    start(server_cfg, pages, rebuild);
}

fn cmd_build(project: &str, out_dir: &str) {
    println!("{}", "🔨 BlissLang Build v0.2".bold().bright_cyan());
    println!("   {}: {}", "Project".dimmed(), project);
    println!("   {}: {}/", "Output".dimmed(), out_dir);
    println!();

    let cfg = BlissConfig::load(project);

    let pf = match load_project(project) {
        Ok(p)  => p,
        Err(e) => { eprintln!("{} {}", "✗".red().bold(), e); std::process::exit(1); }
    };

    let pages = build_pages(&pf);
    fs::create_dir_all(out_dir).expect("Cannot create output directory");

    for (route, html) in &pages {
        let file_path = if route == "/" {
            format!("{}/index.html", out_dir)
        } else {
            let clean = route.trim_start_matches('/');
            let dir   = format!("{}/{}", out_dir, clean);
            fs::create_dir_all(&dir).ok();
            format!("{}/index.html", dir)
        };
        fs::write(&file_path, html).expect("Cannot write HTML");
        println!("  {} {}", "✓".green(), file_path);
    }

    // Write the runtime JS to dist as well
    let runtime = runtime::server::runtime_js_static();
    fs::write(format!("{}/_bliss_runtime.js", out_dir), runtime)
        .expect("Cannot write runtime JS");
    println!("  {} {}/_bliss_runtime.js", "✓".green(), out_dir);

    write_pwa_artifacts(&cfg, out_dir);

    println!();
    println!("{} {} pages built → {}/",
        "✓".green().bold(), pages.len(), out_dir);
}


fn cmd_tokens(file_path: &str) {
    let source = fs::read_to_string(file_path)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    match lexer::tokenize(&source) {
        Err(e) => eprintln!("{}", e),
        Ok(tokens) => {
            let line_len = tokens.iter().map(|t| t.line.to_string().len()).max().unwrap_or(1);
            println!("{} {} — {} tokens\n{}", "Token stream:".bold(), file_path.cyan(), tokens.len(), "─".repeat(55).dimmed());
            for tok in &tokens {
                let kind_str = format!("{}", tok.kind);
                println!("  {:>width$}:{:<4} {}",
                    tok.line, tok.col,
                    kind_str.cyan(),
                    width = line_len
                );
            }
            println!("{}", "─".repeat(55).dimmed());
            println!("{} tokens", tokens.len());
        }
    }
}

fn cmd_ast(file_path: &str) {
    let source = fs::read_to_string(file_path)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
    match lexer::tokenize(&source) {
        Err(e) => eprintln!("Lex error: {}", e),
        Ok(t)  => match parser::parse(t) {
            Err(e) => eprintln!("Parse error: {}", e),
            Ok(a)  => println!("{:#?}", a),
        }
    }
}

fn cmd_info(project: &str) {
    println!("{} {}", "Project info:".bold(), project.cyan());
    println!();

    let pf = match load_project(project) {
        Ok(p)  => p,
        Err(e) => { eprintln!("{} {}", "✗".red(), e); std::process::exit(1); }
    };

    println!("{}", "Pages:".bold());
    for (route, page) in &pf.pages {
        println!("  {}  {}", route.cyan(), page.name.dimmed());
    }

    println!("\n{}", "Sections:".bold());
    for name in pf.sections.keys() {
        println!("  {}", name.cyan());
    }

    if !pf.divs.is_empty() {
        println!("\n{}", "Divs:".bold());
        for name in pf.divs.keys() {
            println!("  {}", name.cyan());
        }
    }

    println!();
    println!("{} {} pages  {} sections  {} divs",
        "Total:".bold(), pf.pages.len(), pf.sections.len(), pf.divs.len());

    let cfg = BlissConfig::load(project);
    match &cfg.pwa {
        Some(pwa) if pwa.enabled => {
            println!("{} {} icon(s), {} strategy, push {}",
                "PWA:".bold().green(),
                pwa.icons.len(),
                pwa.cache_strategy.as_str(),
                if pwa.push_notifications { "enabled" } else { "disabled" });
        }
        _ => println!("{} not enabled (no pwa.enabled: true in bliss.config)", "PWA:".bold().dimmed()),
    }
}

// ─── Project Loader ───────────────────────────────────────────────────────────

pub struct ProjectFiles {
    pub pages:    HashMap<String, PageNode>,
    pub sections: HashMap<String, SectionNode>,
    pub divs:     HashMap<String, DivNode>,
    pub states:   HashMap<String, StateNode>,
}

pub fn load_project(project: &str) -> Result<ProjectFiles, String> {
    let mut pages    = HashMap::new();
    let mut sections = HashMap::new();
    let mut divs     = HashMap::new();
    let mut states   = HashMap::new();
    let mut errors   = Vec::new();
    let mut file_count = 0;

    for entry in WalkDir::new(project).follow_links(true) {
        let entry = match entry {
            Ok(e)  => e,
            Err(e) => { errors.push(e.to_string()); continue; }
        };

        if !entry.file_type().is_file() { continue; }

        let path = entry.path();
        let ext  = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if !matches!(ext, "page"|"section"|"div"|"article"|"state"|"animation") {
            continue;
        }

        file_count += 1;
        let source = match fs::read_to_string(path) {
            Ok(s)  => s,
            Err(e) => {
                errors.push(format!("{}: {}", path.display(), e));
                continue;
            }
        };

        let display = path.strip_prefix(project)
            .unwrap_or(path)
            .display()
            .to_string();

        print!("  {} {} ... ", "→".dimmed(), display.dimmed());

        let tokens = match lexer::tokenize(&source) {
            Ok(t)  => t,
            Err(e) => {
                println!("{}", "✗ lex error".red());
                errors.push(format!("{}: Lex error: {}", path.display(), e));
                continue;
            }
        };

        let ast = match parser::parse(tokens) {
            Ok(a)  => a,
            Err(e) => {
                println!("{}", "✗ parse error".red());
                errors.push(format!("{}: Parse error: {}", path.display(), e));
                continue;
            }
        };

        println!("{}", "✓".green());

        match ast {
            BlissFile::Page(p) => {
                // Layout pages (is_layout: true) are stored under their name as
                // key so layout: "MainLayout" can resolve them, but they don't
                // get a public route of their own.
                let route = if p.is_layout {
                    format!("__layout__{}", p.name)
                } else {
                    p.route.clone().unwrap_or_else(|| {
                        let n = p.name.to_lowercase();
                        if n == "landing" || n == "home" || n == "index" { "/".to_string() }
                        else { format!("/{}", n) }
                    })
                };
                pages.insert(route, p);
            }
            BlissFile::Section(s) => { sections.insert(s.name.clone(), s); }
            BlissFile::Div(d)     => { divs.insert(d.name.clone(), d); }
            BlissFile::State(s)   => { states.insert(s.name.clone(), s); }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    if file_count == 0 {
        return Err(format!("No BlissLang files found in '{}'", project));
    }

    Ok(ProjectFiles { pages, sections, divs, states })
}

// ─── Build ────────────────────────────────────────────────────────────────────

pub fn build_pages(pf: &ProjectFiles) -> HashMap<String, String> {
    let config = RenderConfig::default();
    pf.pages.iter()
        .filter(|(_, page)| !page.is_layout)
        .map(|(route, page)| {
            let html = Renderer::render_page(page, &pf.pages, &pf.sections, &pf.divs, &config, &pf.states);
            (route.clone(), html)
        })
        .collect()
}

/// Build pages tagged with their output mode for the runtime server router.
pub fn build_pages_routed(pf: &ProjectFiles) -> HashMap<String, (String, compiler::ast::OutputMode)> {
    let config = RenderConfig::default();
    pf.pages.iter()
        .filter(|(_, page)| !page.is_layout)
        .map(|(route, page)| {
            let html = Renderer::render_page(page, &pf.pages, &pf.sections, &pf.divs, &config, &pf.states);
            (route.clone(), (html, page.output.clone()))
        })
        .collect()
}

/// Build pages and return (original, minified) pairs for release command.
pub fn build_pages_minified(pf: &ProjectFiles, _cfg: &BlissConfig) -> HashMap<String, (String, String)> {
    let config = RenderConfig::default();
    pf.pages.iter()
        .filter(|(_, page)| !page.is_layout)
        .map(|(route, page)| {
            let html     = Renderer::render_page(page, &pf.pages, &pf.sections, &pf.divs, &config, &pf.states);
            let minified = minify_html(&html);
            (route.clone(), (html, minified))
        })
        .collect()
}

/// Minify HTML — removes redundant whitespace between tags.
pub fn minify_html(html: &str) -> String {
    let mut result    = String::with_capacity(html.len());
    let mut in_pre    = false;
    let mut in_script = false;
    let mut prev_char = ' ';

    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            // Collect tag
            let mut tag = String::from('<');
            for c in chars.by_ref() {
                tag.push(c);
                if c == '>' { break; }
            }
            let tag_lower = tag.to_lowercase();
            if tag_lower.starts_with("<pre") { in_pre    = true; }
            if tag_lower.starts_with("</pre"){ in_pre    = false; }
            if tag_lower.starts_with("<script") { in_script = true; }
            if tag_lower.starts_with("</script"){ in_script = false; }
            result.push_str(&tag);
            prev_char = '>';
        } else if (ch == ' ' || ch == '\n' || ch == '\r' || ch == '\t') && !in_pre && !in_script {
            // Collapse whitespace between tags
            if prev_char != ' ' && prev_char != '>' {
                result.push(' ');
                prev_char = ' ';
            } else if prev_char == '>' {
                // Skip whitespace after closing tag
            }
        } else {
            result.push(ch);
            prev_char = ch;
        }
    }
    result
}

/// Minify JS — remove single-line comments and collapse whitespace.
pub fn minify_js(js: &str) -> String {
    let mut result  = String::with_capacity(js.len());
    let mut in_str  = false;
    let mut str_ch  = '"';
    let mut i       = 0;
    let chars: Vec<char> = js.chars().collect();

    while i < chars.len() {
        let ch = chars[i];

        if in_str {
            result.push(ch);
            if ch == str_ch && (i == 0 || chars[i-1] != '\\') { in_str = false; }
            i += 1;
            continue;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            in_str = true;
            str_ch = ch;
            result.push(ch);
            i += 1;
            continue;
        }

        // Single-line comment
        if ch == '/' && i + 1 < chars.len() && chars[i+1] == '/' {
            while i < chars.len() && chars[i] != '\n' { i += 1; }
            result.push('\n');
            continue;
        }

        // Collapse multiple whitespace to single space
        if ch == '\n' || ch == '\r' || ch == '\t' {
            if result.ends_with(' ') || result.ends_with('\n') {
            } else {
                result.push(' ');
            }
            i += 1;
            continue;
        }

        result.push(ch);
        i += 1;
    }
    result
}

fn cmd_check(file_path: &str) {
    println!("{} {}", "Checking:".bold(), file_path.cyan());

    let source = match fs::read_to_string(file_path) {
        Ok(s)  => s,
        Err(e) => { eprintln!("{} Cannot read: {}", "✗".red(), e); std::process::exit(1); }
    };

    // Lex
    let tokens = match lexer::tokenize(&source) {
        Ok(t)  => { println!("  {} {} tokens", "✓".green(), t.len()); t }
        Err(e) => { eprintln!("  {} Lex error: {}", "✗".red(), e); std::process::exit(1); }
    };

    // Parse
    let ast = match parser::parse(tokens) {
        Ok(a)  => { println!("  {} Parsed as {}", "✓".green(), ast_kind_name(&a).bold()); a }
        Err(e) => { eprintln!("  {} Parse error: {}", "✗".red(), e); std::process::exit(1); }
    };

    // Type check
    let (errors, warnings) = typechecker::check_project(&[ast], &[], &[]);
    for w in &warnings {
        println!("  {} {}", "⚠".yellow(), w.message.dimmed());
    }
    for e in &errors {
        eprintln!("  {} {}", "✗".red(), e.message);
    }

    if !errors.is_empty() {
        eprintln!("  {} {} type error(s)", "✗".red(), errors.len());
        std::process::exit(1);
    }

    if warnings.is_empty() && errors.is_empty() {
        println!("  {} No syntax or type errors", "✓".green());
    }
    println!("{}", "All checks passed.".green().bold());
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ast_kind_name(ast: &BlissFile) -> &'static str {
    match ast {
        BlissFile::Page(_)      => "Page",
        BlissFile::Section(_)   => "Section",
        BlissFile::Div(_)       => "Div",
        BlissFile::Article(_)   => "Article",
        BlissFile::State(_)     => "State",
        BlissFile::Model(_)     => "Model",
        BlissFile::Animation(_) => "Animation",
        BlissFile::TypeDef(_)   => "TypeDef",
        BlissFile::ApiRoute(_)  => "ApiRoute",
    }
}

// ─── Banner ───────────────────────────────────────────────────────────────────

fn print_banner() {
    println!();
    println!("{}", "  ╔════════════════════════════════════════════╗".bright_red());
    println!("{}", "  ║   B L I S S L A N G    v 0 . 3            ║".bright_red());
    println!("{}", "  ║   Build websites section by section        ║".bright_red());
    println!("{}", "  ║   Zero npm  •  Zero axum  •  Pure Rust     ║".bright_red());
    println!("{}", "  ╚════════════════════════════════════════════╝".bright_red());
    println!("  {}  {}",
        "Bliss Mahlathi".bold(),
        "PulseBit — Nkowankowa, Limpopo".dimmed()
    );
    println!();
}
