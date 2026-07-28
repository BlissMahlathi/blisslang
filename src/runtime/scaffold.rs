/// BlissLang Scaffolding — `bliss new <name>`
///
/// Creates a complete BlissLang project structure.
/// All template strings use .to_string() + .replace() to avoid
/// format! escaping issues with Tailwind class prefixes like hover:

use colored::Colorize;
use std::fs;
use std::path::Path;

pub struct ScaffoldOptions {
    pub name:     String,
    pub template: Template,
}

pub enum Template {
    Landing,
    Dashboard,
    Minimal,
}

impl Template {
    pub fn from_str(s: &str) -> Self {
        match s {
            "dashboard" => Template::Dashboard,
            "minimal"   => Template::Minimal,
            _           => Template::Landing,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Template::Landing   => "landing",
            Template::Dashboard => "dashboard",
            Template::Minimal   => "minimal",
        }
    }
}

pub fn scaffold(opts: ScaffoldOptions) -> Result<(), String> {
    let root = &opts.name;
    if Path::new(root).exists() {
        return Err(format!("Directory '{}' already exists", root));
    }

    println!("{} {} ({})",
        "Creating".green().bold(),
        root.cyan().bold(),
        opts.template.label().dimmed()
    );
    println!();

    let dirs = vec![
        format!("{}/Pages/LandingPage", root),
        format!("{}/Sections",          root),
        format!("{}/Divs",              root),
        format!("{}/Articles",          root),
        format!("{}/Assets/Images",     root),
        format!("{}/Assets/Videos",     root),
        format!("{}/Assets/Fonts",      root),
        format!("{}/Locales",           root),
        format!("{}/State",             root),
        format!("{}/Types",             root),
        format!("{}/Animations",        root),
        format!("{}/Tests",             root),
        format!("{}/.bliss",            root),
    ];

    for dir in &dirs {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Cannot create {}: {}", dir, e))?;
        println!("  {} {}/", "📁".dimmed(), dir);
    }
    println!();

    let files = build_files(root, &opts.name, &opts.template);
    for (path, content) in &files {
        fs::write(path, content)
            .map_err(|e| format!("Cannot write {}: {}", path, e))?;
        println!("  {} {}", "✓".green(), path);
    }

    println!();
    println!("{}", "Project ready!".green().bold());
    println!();
    println!("  {}", "Next steps:".bold());
    println!("  {} cd {}", "1.".dimmed(), root.cyan());
    println!("  {} bliss dev", "2.".dimmed());
    println!();
    println!("  {} http://localhost:4578", "->".cyan());
    println!();
    Ok(())
}

fn build_files(root: &str, name: &str, template: &Template) -> Vec<(String, String)> {
    let mut files = Vec::new();

    files.push((format!("{}/bliss.config", root), bliss_config(name)));
    files.push((format!("{}/.gitignore",   root), gitignore()));
    files.push((format!("{}/README.md",    root), readme(name)));

    match template {
        Template::Landing | Template::Minimal => {
            let minimal = matches!(template, Template::Minimal);
            files.extend(landing_files(root, name, minimal));
        }
        Template::Dashboard => {
            files.extend(dashboard_files(root, name));
        }
    }

    files.extend(shared_files(root, name));
    files
}

// ── bliss.config ─────────────────────────────────────────────────────────────

fn bliss_config(name: &str) -> String {
    r##"# BlissLang Configuration
project:       "NAME"
version:       "0.1.0"
output:        "static"
port:          4578
hot_reload:    true
tailwind:      true
animations:    true
geometry:      true
signals:       true
svg:           "bliss"
security:      "strict"
i18n:
    default:   "en"
    locales:   ["en", "zu"]
    fallback:  "en"
out_dir:       "dist"
"##.to_string().replace("NAME", name)
}

// ── Landing template files ────────────────────────────────────────────────────

fn landing_files(root: &str, name: &str, minimal: bool) -> Vec<(String, String)> {
    let mut f = Vec::new();

    f.push((
        format!("{}/Pages/LandingPage/Landing.page", root),
        landing_page(name, minimal),
    ));
    f.push((
        format!("{}/Sections/Hero.section", root),
        hero_section(name, minimal),
    ));
    if !minimal {
        f.push((format!("{}/Sections/Features.section", root), features_section()));
        f.push((format!("{}/Divs/Card.div",             root), card_div()));
        f.push((format!("{}/State/AppState.state",      root), app_state()));
    }
    f.push((format!("{}/Sections/Footer.section", root), footer_section(name)));
    f
}

// ── Dashboard template files ──────────────────────────────────────────────────

fn dashboard_files(root: &str, name: &str) -> Vec<(String, String)> {
    vec![
        (format!("{}/Pages/LandingPage/Landing.page", root), dashboard_page(name)),
        (format!("{}/Sections/NavBar.section",        root), navbar_section(name)),
        (format!("{}/Sections/Dashboard.section",     root), dashboard_section()),
        (format!("{}/Sections/Footer.section",        root), footer_section(name)),
        (format!("{}/Divs/StatCard.div",              root), stat_card_div()),
        (format!("{}/State/AppState.state",           root), app_state()),
    ]
}

// ── Shared files ──────────────────────────────────────────────────────────────

fn shared_files(root: &str, name: &str) -> Vec<(String, String)> {
    vec![
        (format!("{}/Locales/en.locale",              root), en_locale(name)),
        (format!("{}/Locales/zu.locale",              root), zu_locale(name)),
        (format!("{}/Animations/Reveal.animation",    root), reveal_animation()),
        (format!("{}/.bliss/cache",                   root), String::new()),
    ]
}

// ── Page templates ────────────────────────────────────────────────────────────

fn landing_page(name: &str, minimal: bool) -> String {
    let sections = if minimal {
        "    IncludeSection[\"Hero\"]\n\n    IncludeSection[\"Footer\"]\n"
    } else {
        "    IncludeSection[\"Hero\"]\n\n    IncludeSection[\"Features\"]\n\n    IncludeSection[\"Footer\"]\n"
    };

    let tmpl = r##"BuildPage[
    name:   "Landing",
    route:  "/",
    title:  "NAME",
    output: "static"
]:

SECTIONS"##;
    tmpl.replace("NAME", name).replace("SECTIONS", sections)
}

fn dashboard_page(name: &str) -> String {
    r##"BuildPage[
    name:   "Dashboard",
    route:  "/",
    title:  "NAME - Dashboard",
    output: "runtime"
]:

    IncludeSection["NavBar"]

    IncludeSection["Dashboard"]

    IncludeSection["Footer"]
"##.to_string().replace("NAME", name)
}

// ── Section templates ─────────────────────────────────────────────────────────

fn hero_section(name: &str, minimal: bool) -> String {
    if minimal {
        r##"BuildSection[
    name:           "Hero",
    style.tailwind: "min-h-screen flex flex-col justify-center items-center bg-white px-6"
]:

    h1[
        text:            "NAME",
        style.tailwind:  "text-5xl font-bold text-slate-900",
        animate:         "fadeInUp",
        animate.trigger: "load"
    ]

    p[
        text:            "Built with BlissLang",
        style.tailwind:  "text-slate-500 mt-4",
        animate:         "fadeInUp",
        animate.delay:   "200ms",
        animate.trigger: "load"
    ]
"##.to_string().replace("NAME", name)
    } else {
        r##"BuildSection[
    name:           "Hero",
    style.tailwind: "bg-slate-900 min-h-screen flex flex-col justify-center items-center px-6"
]:

    h1[
        text:            "NAME",
        style.tailwind:  "text-6xl font-bold text-white text-center",
        animate:         "fadeInUp",
        animate.delay:   "0ms",
        animate.trigger: "load"
    ]

    p[
        text:            "Built with BlissLang — Build websites section by section",
        style.tailwind:  "text-xl text-blue-200 mt-4 max-w-2xl text-center",
        animate:         "fadeInUp",
        animate.delay:   "200ms",
        animate.trigger: "load"
    ]

    div[
        style.tailwind:  "flex gap-4 mt-10",
        animate:         "fadeInUp",
        animate.delay:   "400ms",
        animate.trigger: "load"
    ]:

        button[
            text:           "Get Started",
            style.tailwind: "px-8 py-3 bg-red-500 text-white rounded-lg font-semibold",
            link:           "#features"
        ]

        button[
            text:           "Learn More",
            style.tailwind: "px-8 py-3 border border-slate-600 text-slate-300 rounded-lg font-semibold",
            link:           "https://blisslang.dev"
        ]
"##.to_string().replace("NAME", name)
    }
}

fn features_section() -> String {
    r##"BuildSection[
    name:           "Features",
    style.tailwind: "bg-white py-24 px-6"
]:

    div[style.tailwind: "max-w-6xl mx-auto"]:

        h2[
            text:            "Why BlissLang?",
            style.tailwind:  "text-4xl font-bold text-center text-slate-900 mb-4",
            animate:         "fadeInUp",
            animate.trigger: "scroll"
        ]

        p[
            text:           "Everything you need. Nothing you don't.",
            style.tailwind: "text-center text-slate-500 text-lg mb-16"
        ]

        div[style.tailwind: "grid grid-cols-1 md:grid-cols-3 gap-8"]:

            UseDiv["Card"][
                icon:        "lock",
                title:       "Zero npm Risk",
                description: "No node_modules. No supply chain attacks. Pure Rust compiler."
            ]

            UseDiv["Card"][
                icon:        "bolt",
                title:       "Rust Powered",
                description: "Millisecond builds. Low memory. Cross-platform. One binary."
            ]

            UseDiv["Card"][
                icon:        "layers",
                title:       "Section Driven",
                description: "Think in pages and sections, not components and hooks."
            ]
"##.to_string()
}

fn footer_section(name: &str) -> String {
    r##"BuildSection[
    name:           "Footer",
    style.tailwind: "bg-slate-950 py-12 px-6"
]:

    div[style.tailwind: "max-w-6xl mx-auto flex flex-col items-center gap-3"]:

        p[
            text:           "NAME",
            style.tailwind: "text-2xl font-bold text-red-500"
        ]

        p[
            text:           "Built with BlissLang",
            style.tailwind: "text-slate-500 text-sm"
        ]

        p[
            text:           "2025",
            style.tailwind: "text-slate-700 text-xs"
        ]
"##.to_string().replace("NAME", name)
}

fn navbar_section(name: &str) -> String {
    r##"BuildSection[
    name:           "NavBar",
    style.tailwind: "bg-slate-900 border-b border-slate-800 px-6 py-4"
]:

    div[style.tailwind: "max-w-7xl mx-auto flex items-center justify-between"]:

        p[
            text:           "NAME",
            style.tailwind: "text-xl font-bold text-red-500"
        ]

        nav[style.tailwind: "flex gap-6"]:
            a[href: "/",         text: "Dashboard", style.tailwind: "text-slate-300"]
            a[href: "/reports",  text: "Reports",   style.tailwind: "text-slate-300"]
            a[href: "/settings", text: "Settings",  style.tailwind: "text-slate-300"]

        button[
            text:           "Logout",
            style.tailwind: "px-4 py-2 bg-red-500 text-white rounded-lg text-sm"
        ]
"##.to_string().replace("NAME", name)
}

fn dashboard_section() -> String {
    r##"BuildSection[
    name:           "Dashboard",
    style.tailwind: "flex-1 p-8 bg-slate-900 min-h-screen"
]:

    h1[
        text:           "Dashboard",
        style.tailwind: "text-3xl font-bold text-white mb-8"
    ]

    div[style.tailwind: "grid grid-cols-1 md:grid-cols-4 gap-6 mb-8"]:

        UseDiv["StatCard"][label: "Total Users",  value: "0",   icon: "person"]
        UseDiv["StatCard"][label: "Revenue",      value: "R0",  icon: "money"]
        UseDiv["StatCard"][label: "Active Today", value: "0",   icon: "bolt"]
        UseDiv["StatCard"][label: "Growth",       value: "0%",  icon: "chart"]

    div[style.tailwind: "bg-slate-800 rounded-2xl p-6 border border-slate-700"]:
        h2[text: "Recent Activity", style.tailwind: "text-xl font-bold text-white mb-4"]
        p[text:  "No activity yet.", style.tailwind: "text-slate-400"]
"##.to_string()
}

// ── Div templates ─────────────────────────────────────────────────────────────

fn card_div() -> String {
    r##"BuildDiv[
    name:           "Card",
    style.tailwind: "bg-slate-50 border border-slate-100 rounded-2xl p-8"
]:

    Props:
        icon:        String, default: "star"
        title:       String, required
        description: String, required

    div[style.tailwind: "flex flex-col"]:

        p[text: "{props.icon}", style.tailwind: "text-4xl mb-4"]

        h3[text: "{props.title}", style.tailwind: "text-xl font-bold text-slate-900 mb-2"]

        p[text: "{props.description}", style.tailwind: "text-slate-500 leading-relaxed"]
"##.to_string()
}

fn stat_card_div() -> String {
    r##"BuildDiv[
    name:           "StatCard",
    style.tailwind: "bg-slate-800 border border-slate-700 rounded-2xl p-6"
]:

    Props:
        label: String, required
        value: String, required
        icon:  String, default: "chart"

    div[style.tailwind: "flex items-start justify-between"]:

        div[]:
            p[text: "{props.label}", style.tailwind: "text-slate-400 text-sm mb-1"]
            p[text: "{props.value}", style.tailwind: "text-3xl font-bold text-white"]

        p[text: "{props.icon}", style.tailwind: "text-3xl"]
"##.to_string()
}

// ── State template ────────────────────────────────────────────────────────────

fn app_state() -> String {
    r##"CreateState[name: "App"]:
    user:       Signal[type: User, default: null]
    isLoggedIn: Signal[type: Bool, default: false]
    theme:      Signal[type: String, default: "dark"]
    cart:       Signal[type: List, default: []]

    cartCount: Derived[from: "cart", compute: "cart.get().length"]
"##.to_string()
}

// ── Locale templates ──────────────────────────────────────────────────────────

fn en_locale(name: &str) -> String {
    r##"[meta]
language:    "English"
direction:   "ltr"
currency:    "ZAR"

[nav]
home:        "Home"
about:       "About"
contact:     "Contact"
login:       "Login"
logout:      "Logout"

[hero]
title:       "NAME"
subtitle:    "Built with BlissLang"
cta:         "Get Started"

[errors]
required:    "This field is required"
network:     "Network error, please try again"
"##.to_string().replace("NAME", name)
}

fn zu_locale(name: &str) -> String {
    r##"[meta]
language:    "isiZulu"
direction:   "ltr"
currency:    "ZAR"

[nav]
home:        "Ikhaya"
about:       "Mayelana"
contact:     "Xhumana"
login:       "Ngena"
logout:      "Phuma"

[hero]
title:       "NAME"
subtitle:    "Yakhelwe nge-BlissLang"
cta:         "Qala"

[errors]
required:    "Le nkambu iyadingeka"
network:     "Iphutha lenethiwekhi, zama futhi"
"##.to_string().replace("NAME", name)
}

// ── Animation template ────────────────────────────────────────────────────────

fn reveal_animation() -> String {
    r##"DefineAnimation[name: "glowReveal"]:

    frame[at: "0%"]:
        opacity:   0
        transform: "scale(0.8)"
        filter:    "blur(8px)"

    frame[at: "60%"]:
        opacity:   1
        transform: "scale(1.02)"
        filter:    "blur(0px)"

    frame[at: "100%"]:
        opacity:   1
        transform: "scale(1)"
"##.to_string()
}

// ── .gitignore and README ─────────────────────────────────────────────────────

fn gitignore() -> String {
    r##"dist/
.bliss/cache
.DS_Store
Thumbs.db
.vscode/
.idea/
*.swp
"##.to_string()
}

fn readme(name: &str) -> String {
    let content = r##"# NAME

Built with BlissLang - a section-driven enterprise web language powered by Rust.

## Getting started

    bliss dev       # Start dev server with hot reload
    bliss build     # Build for production
    bliss check Sections/Hero.section

## Project structure

    NAME/
      Pages/        # URL-mapped pages
      Sections/     # Reusable sections
      Divs/         # Reusable components
      State/        # Reactive state
      Assets/       # Images, fonts, videos
      Locales/      # Translation files
      bliss.config  # Project configuration

## Learn more

  https://blisslang.dev
  https://pulsebit.dev
"##;
    content.to_string().replace("NAME", name)
}
