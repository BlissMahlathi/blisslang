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
use runtime::server::{ServerConfig, start};

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
        Command::Dev { project, port, no_reload, threads } => {
            cmd_dev(&project, port, !no_reload, threads);
        }
        Command::Build { project, out } => {
            cmd_build(&project, &out);
        }
        Command::Check { file }  => cmd_check(&file),
        Command::Tokens { file } => cmd_tokens(&file),
        Command::Ast { file }    => cmd_ast(&file),
        Command::Info { project } => cmd_info(&project),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

fn cmd_dev(project: &str, port: u16, hot_reload: bool, threads: usize) {
    println!("{}", "🚀 BlissLang Dev Server v0.2".bold().bright_cyan());
    println!("   {}: {}", "Project".dimmed(), project);
    println!("   {}: http://localhost:{}", "URL".dimmed(), port);
    println!("   {}: std::net (zero external HTTP deps)", "Server".dimmed());
    println!("   {}: {}", "Threads".dimmed(), threads);
    println!("   {}: {}", "Hot reload".dimmed(),
        if hot_reload { "enabled (SSE)".green().to_string() }
        else          { "disabled".dimmed().to_string() }
    );
    println!();

    // Initial build
    let project_str = project.to_string();
    let pf = match load_project(&project_str) {
        Ok(p)  => p,
        Err(e) => { eprintln!("{} {}", "✗".red().bold(), e); std::process::exit(1); }
    };

    let pages = build_pages(&pf);
    println!();
    println!("{} {} pages  {} sections  {} divs",
        "✓".green().bold(), pf.pages.len(), pf.sections.len(), pf.divs.len());
    println!();
    println!("{}", "Routes:".bold());
    for route in pages.keys() {
        println!("  {} http://localhost:{}{}", "→".cyan(), port, route);
    }
    println!();

    let config = ServerConfig {
        port,
        host:       "127.0.0.1".into(),
        hot_reload,
        project:    project_str.clone(),
        threads,
    };

    // The rebuild closure — called by the watcher on file changes
    let rebuild = move |proj: &str| -> HashMap<String, String> {
        match load_project(proj) {
            Ok(pf) => build_pages(&pf),
            Err(e) => {
                eprintln!("  {} Rebuild failed: {}", "✗".red(), e);
                HashMap::new()
            }
        }
    };

    println!("{}", format!("Listening on http://localhost:{}", port).green().bold());
    println!("{}", "Press Ctrl+C to stop".dimmed());
    println!();

    // Blocks here — serves forever
    start(config, pages, rebuild);
}

fn cmd_build(project: &str, out_dir: &str) {
    println!("{}", "🔨 BlissLang Build v0.2".bold().bright_cyan());
    println!("   {}: {}", "Project".dimmed(), project);
    println!("   {}: {}/", "Output".dimmed(), out_dir);
    println!();

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

    println!();
    println!("{} {} pages built → {}/",
        "✓".green().bold(), pages.len(), out_dir);
}

fn cmd_check(file_path: &str) {
    println!("{} {}", "Checking:".bold(), file_path.cyan());

    let source = match fs::read_to_string(file_path) {
        Ok(s)  => s,
        Err(e) => { eprintln!("{} Cannot read: {}", "✗".red(), e); std::process::exit(1); }
    };

    // Lex
    let tokens = match lexer::tokenize(&source) {
        Ok(t)  => { println!("  {} Lexed {} tokens", "✓".green(), t.len()); t }
        Err(e) => {
            eprintln!("  {} Lex error: {}", "✗".red(), e);
            std::process::exit(1);
        }
    };

    // Parse
    match parser::parse(tokens) {
        Ok(ast) => {
            let kind = ast_kind_name(&ast);
            println!("  {} Parsed as {}", "✓".green(), kind.bold());
            println!("  {} No syntax errors", "✓".green());
            println!("{}", "All checks passed.".green().bold());
        }
        Err(e) => {
            eprintln!("  {} Parse error: {}", "✗".red(), e);
            std::process::exit(1);
        }
    }
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
}

// ─── Project Loader ───────────────────────────────────────────────────────────

pub struct ProjectFiles {
    pub pages:    HashMap<String, PageNode>,
    pub sections: HashMap<String, SectionNode>,
    pub divs:     HashMap<String, DivNode>,
}

pub fn load_project(project: &str) -> Result<ProjectFiles, String> {
    let mut pages    = HashMap::new();
    let mut sections = HashMap::new();
    let mut divs     = HashMap::new();
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
                let route = p.route.clone()
                    .unwrap_or_else(|| {
                        let n = p.name.to_lowercase();
                        if n == "landing" || n == "home" || n == "index" { "/".to_string() }
                        else { format!("/{}", n) }
                    });
                pages.insert(route, p);
            }
            BlissFile::Section(s) => { sections.insert(s.name.clone(), s); }
            BlissFile::Div(d)     => { divs.insert(d.name.clone(), d); }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    if file_count == 0 {
        return Err(format!("No BlissLang files found in '{}'", project));
    }

    Ok(ProjectFiles { pages, sections, divs })
}

// ─── Build ────────────────────────────────────────────────────────────────────

pub fn build_pages(pf: &ProjectFiles) -> HashMap<String, String> {
    let config = RenderConfig::default();
    pf.pages.iter()
        .map(|(route, page)| {
            let html = Renderer::render_page(page, &pf.sections, &pf.divs, &config);
            (route.clone(), html)
        })
        .collect()
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
    println!("{}", "  ║   B L I S S L A N G    v 0 . 2            ║".bright_red());
    println!("{}", "  ║   Build websites section by section        ║".bright_red());
    println!("{}", "  ║   Zero npm  •  Zero axum  •  Pure Rust     ║".bright_red());
    println!("{}", "  ╚════════════════════════════════════════════╝".bright_red());
    println!("  {}  {}",
        "Bliss Mahlathi".bold(),
        "PulseBit — Nkowankowa, Limpopo".dimmed()
    );
    println!();
}
