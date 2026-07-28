/// BlissLang Type Checker — v0.5
///
/// Validates every attribute value in the AST at compile time.
/// Catches: unknown attributes, wrong value types, Tailwind class typos,
/// missing required props, invalid animate values, bad asset paths.
///
/// Runs after parsing, before rendering.
/// Returns Vec<TypeError> — build fails if any Error-level issues found.

use crate::compiler::ast::*;
use crate::compiler::style::lookup as tailwind_lookup;
// HashMap reserved for v0.6 cached lookups

// ─── Type Error ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Severity { Error, Warning, Info }

#[derive(Debug, Clone)]
pub struct TypeError {
    pub severity: Severity,
    pub location: String,   // e.g. "Hero.section:h1[animate]"
    pub message:  String,
}

impl TypeError {
    fn error(loc: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Error,   location: loc.into(), message: msg.into() }
    }
    fn warn(loc: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { severity: Severity::Warning, location: loc.into(), message: msg.into() }
    }
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            Severity::Error   => "ERROR",
            Severity::Warning => "WARN ",
            Severity::Info    => "INFO ",
        };
        write!(f, "[{}] {} — {}", prefix, self.location, self.message)
    }
}

// ─── Type Checker ─────────────────────────────────────────────────────────────

pub struct TypeChecker {
    errors:   Vec<TypeError>,
    sections: Vec<String>,  // known section names
    divs:     Vec<String>,  // known div names
}

impl TypeChecker {
    pub fn new(sections: Vec<String>, divs: Vec<String>) -> Self {
        Self { errors: Vec::new(), sections, divs }
    }

    pub fn check_file(&mut self, file: &BlissFile) {
        match file {
            BlissFile::Page(p)      => self.check_page(p),
            BlissFile::Section(s)   => self.check_section(s),
            BlissFile::Div(d)       => self.check_div(d),
            BlissFile::State(s)     => self.check_state(s),
            BlissFile::Animation(a) => self.check_animation(a),
            _                       => {}
        }
    }

    pub fn into_errors(self) -> Vec<TypeError> {
        self.errors
    }

    // ── Page ──────────────────────────────────────────────────────────────

    fn check_page(&mut self, page: &PageNode) {
        let loc = format!("{}.page", page.name);

        // Validate known page attributes
        for attr in &page.attrs {
            let key = attr.key_str();
            match key.as_str() {
                "name" | "route" | "title" | "output" | "auth" | "meta.description"
                | "meta.keywords" | "meta.og.title" | "meta.og.image"
                | "lang" | "charset" | "pwa.offline" => {}
                other => {
                    self.errors.push(TypeError::warn(
                        &loc,
                        format!("Unknown page attribute: '{}'", other)
                    ));
                }
            }
        }

        // The PWA offline fallback page must be static — it has to render
        // with zero network access, so a runtime/hybrid page can't serve it.
        let is_offline_page = page.attrs.iter().any(|a| {
            a.key_str() == "pwa.offline" && matches!(&a.value, AttrValue::Bool(true))
        });
        if is_offline_page && page.output != OutputMode::Static {
            self.errors.push(TypeError::error(
                &loc,
                format!(
                    "page marked pwa.offline: true must have output: \"static\" (found \"{}\") — it needs to render with no network access",
                    page.output.as_str()
                ),
            ));
        }

        // Check section references exist
        for child in &page.sections {
            if let PageChild::Include { name, .. } = child {
                if !self.sections.contains(name) {
                    self.errors.push(TypeError::error(
                        &loc,
                        format!("IncludeSection[\"{}\"]: section not found in project", name)
                    ));
                }
            }
        }
    }

    // ── Section ───────────────────────────────────────────────────────────

    fn check_section(&mut self, section: &SectionNode) {
        let loc = format!("{}.section", section.name);
        self.check_attrs(&section.attrs, &loc, "BuildSection");
        for child in &section.children {
            self.check_child(child, &loc);
        }
    }

    // ── Div ───────────────────────────────────────────────────────────────

    fn check_div(&mut self, div: &DivNode) {
        let loc = format!("{}.div", div.name);
        self.check_attrs(&div.attrs, &loc, "BuildDiv");
        for child in &div.children {
            self.check_child(child, &loc);
        }
    }

    // ── State ─────────────────────────────────────────────────────────────

    fn check_state(&mut self, state: &StateNode) {
        let loc = format!("{}.state", state.name);
        let valid_types = ["String","Int","Decimal","Bool","Date","DateTime",
                           "Url","Email","Phone","UUID","Json","List","Map",
                           "User","Any","Signal","Derived"];
        for sig in &state.signals {
            if !valid_types.contains(&sig.ty.as_str()) {
                self.errors.push(TypeError::warn(
                    &loc,
                    format!("Signal '{}' has unknown type '{}' — will be treated as Any", sig.name, sig.ty)
                ));
            }
        }
    }

    // ── Animation ─────────────────────────────────────────────────────────

    fn check_animation(&mut self, anim: &AnimationNode) {
        let loc = format!("{}.animation", anim.name);
        if anim.frames.is_empty() {
            self.errors.push(TypeError::error(&loc, "Animation has no frames"));
        }
        for frame in &anim.frames {
            let at = frame.at.trim_end_matches('%');
            let valid = matches!(frame.at.as_str(), "from" | "to")
                || at.parse::<u8>().map(|n| n <= 100).unwrap_or(false);
            if !valid {
                self.errors.push(TypeError::error(
                    &loc,
                    format!("Frame 'at' value '{}' is invalid — use 0%-100%, 'from', or 'to'", frame.at)
                ));
            }
        }
    }

    // ── Child nodes ───────────────────────────────────────────────────────

    fn check_child(&mut self, child: &Child, parent_loc: &str) {
        match child {
            Child::Element(el) => {
                let loc = format!("{}[{}]", parent_loc, el.tag);
                self.check_element(el, &loc);
            }
            Child::UseDiv { name, children, .. } => {
                let loc = format!("{}[UseDiv:{}]", parent_loc, name);
                if !self.divs.contains(name) {
                    self.errors.push(TypeError::error(
                        &loc,
                        format!("UseDiv[\"{}\"] — div not found in project", name)
                    ));
                }
                for child in children {
                    self.check_child(child, &loc);
                }
            }
            Child::ForEach { collection, binding, body, .. } => {
                let loc = format!("{}[ForEach:{}]", parent_loc, collection);
                if collection.trim().is_empty() {
                    self.errors.push(TypeError::error(&loc, "ForEach collection path is empty"));
                }
                if binding.trim().is_empty() {
                    self.errors.push(TypeError::error(&loc, "ForEach binding name is empty"));
                }
                for child in body {
                    self.check_child(child, &loc);
                }
            }
            Child::ShowIf { cond, then, else_ } => {
                if cond.trim().is_empty() {
                    self.errors.push(TypeError::error(
                        parent_loc,
                        "ShowIf condition is empty"
                    ));
                }
                for child in then  { self.check_child(child, parent_loc); }
                for child in else_ { self.check_child(child, parent_loc); }
            }
            Child::GeoCanvas { attrs, children } => {
                let loc = format!("{}[DrawCanvas]", parent_loc);
                self.check_geo_canvas(attrs, children, &loc);
            }
            Child::ErrorBoundary { body, .. } => {
                for child in body { self.check_child(child, parent_loc); }
            }
            Child::Into { children, .. } => {
                for child in children { self.check_child(child, parent_loc); }
            }
            Child::Responsive { body, .. } => {
                for child in body { self.check_child(child, parent_loc); }
            }
            _ => {}
        }
    }

    // ── Element validation ────────────────────────────────────────────────

    fn check_element(&mut self, el: &ElementNode, loc: &str) {
        // Validate tag name
        if !is_valid_html5_tag(&el.tag) {
            self.errors.push(TypeError::error(
                loc,
                format!("'{}' is not a valid HTML5 element — check spelling", el.tag)
            ));
        }

        self.check_attrs(&el.attrs, loc, &el.tag);

        for child in &el.children {
            self.check_child(child, loc);
        }
    }

    // ── Attribute validation ──────────────────────────────────────────────

    fn check_attrs(&mut self, attrs: &AttrList, loc: &str, context: &str) {
        for attr in attrs {
            let key = attr.key_str();
            self.check_attr_value(&key, &attr.value, loc, context);
        }
    }

    fn check_attr_value(&mut self, key: &str, value: &AttrValue, loc: &str, context: &str) {
        match key {
            // ── Style checks ──────────────────────────────────────────────
            "style.tailwind" => {
                if let AttrValue::Str(classes) = value {
                    self.check_tailwind_classes(classes, loc);
                }
            }

            "style.css" => {
                if let AttrValue::Str(css) = value {
                    if css.contains('<') || css.contains('>') {
                        self.errors.push(TypeError::error(
                            loc,
                            "style.css contains HTML characters — use plain CSS properties"
                        ));
                    }
                }
            }

            // ── Animation checks ─────────────────────────────────────────
            "animate" => {
                if let AttrValue::Str(name) = value {
                    let valid_presets = [
                        "fadeIn","fadeInUp","fadeInDown","fadeInLeft","fadeInRight",
                        "slideInUp","slideInLeft","slideInRight",
                        "zoomIn","zoomInUp","bounceIn","flipInX","flipInY","rotateIn",
                        "pulse","shake","bounce","spin","ping",
                        "fadeOut","slideOutLeft","zoomOut",
                    ];
                    if !valid_presets.contains(&name.as_str()) {
                        self.errors.push(TypeError::warn(
                            loc,
                            format!("animate: \"{}\" is not a built-in preset — ensure it is defined in Animations/", name)
                        ));
                    }
                }
            }

            "animate.trigger" => {
                if let AttrValue::Str(t) = value {
                    if !matches!(t.as_str(), "scroll" | "load" | "hover" | "click") {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("animate.trigger: \"{}\" is invalid — use scroll | load | hover | click", t)
                        ));
                    }
                }
            }

            "animate.delay" | "animate.duration" => {
                if let AttrValue::Str(t) = value {
                    if !t.ends_with("ms") && !t.ends_with('s') {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("{}: \"{}\" must be a CSS time value — e.g. '300ms' or '0.3s'", key, t)
                        ));
                    }
                }
            }

            // ── Asset path checks ─────────────────────────────────────────
            "src" => {
                if let AttrValue::Str(path) = value {
                    if !path.starts_with("http") && !path.starts_with('@')
                        && !path.starts_with('/') && !path.starts_with("data:")
                        && !path.contains("://") && !path.is_empty()
                    {
                        self.errors.push(TypeError::warn(
                            loc,
                            format!("src: \"{}\" — relative paths may not resolve. Use @Images/file.png or a full URL", path)
                        ));
                    }
                }
            }

            // ── Link checks ───────────────────────────────────────────────
            "href" | "link" => {
                if let AttrValue::Str(href) = value {
                    if href.contains("javascript:") {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("{}: \"{}\" — javascript: URIs are blocked by BlissLang security", key, href)
                        ));
                    }
                }
            }

            // ── Output mode check (page-level) ────────────────────────────
            "output" => {
                if let AttrValue::Str(mode) = value {
                    if !matches!(mode.as_str(), "static" | "runtime" | "hybrid") {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("output: \"{}\" is invalid — use static | runtime | hybrid", mode)
                        ));
                    }
                }
            }

            // ── Type attribute on inputs ──────────────────────────────────
            "type" if context == "input" => {
                if let AttrValue::Str(t) = value {
                    let valid_input_types = [
                        "text","email","password","number","tel","url","search",
                        "date","time","datetime-local","month","week","color",
                        "range","checkbox","radio","file","hidden","submit",
                        "reset","button","image",
                    ];
                    if !valid_input_types.contains(&t.as_str()) {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("input type: \"{}\" is not a valid HTML5 input type", t)
                        ));
                    }
                }
            }

            // ── Loading attribute on img ──────────────────────────────────
            "loading" => {
                if let AttrValue::Str(l) = value {
                    if !matches!(l.as_str(), "lazy" | "eager" | "auto") {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("loading: \"{}\" is invalid — use lazy | eager | auto", l)
                        ));
                    }
                }
            }

            // ── method on form ────────────────────────────────────────────
            "method" => {
                if let AttrValue::Str(m) = value {
                    if !matches!(m.to_uppercase().as_str(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                        self.errors.push(TypeError::error(
                            loc,
                            format!("method: \"{}\" is invalid — use GET | POST | PUT | PATCH | DELETE", m)
                        ));
                    }
                }
            }

            // ── Known safe passthrough attributes ─────────────────────────
            "name" | "id" | "class" | "text" | "placeholder" | "value" | "alt"
            | "width" | "height" | "rows" | "cols" | "maxlength" | "minlength"
            | "min" | "max" | "step" | "pattern" | "autocomplete" | "autofocus"
            | "disabled" | "readonly" | "required" | "checked" | "selected"
            | "multiple" | "action" | "enctype" | "novalidate"
            | "target" | "rel" | "download" | "hreflang"
            | "autoplay" | "controls" | "loop" | "muted" | "preload" | "poster"
            | "style.scss" | "style.theme"
            | "reactive" | "show" | "slot" | "onclick" | "onchange"
            | "oninput" | "onsubmit" | "onfocus" | "onblur"
            | "role" | "tabindex" | "draggable" | "contenteditable"
            | "hidden" | "translate" | "spellcheck" | "title" | "lang" | "dir"
            | "geo.animate" | "geo.duration" | "geo.repeat"
            | "data-reactive" | "data-model" | "data-onclick"
            => {}

            // ── data.* passthrough ────────────────────────────────────────
            k if k.starts_with("data.") => {}
            k if k.starts_with("aria.") => {}

            // ── BlissGeo-specific ─────────────────────────────────────────
            "center" | "radius" | "fill" | "border.color" | "border.width"
            | "at" | "from" | "to" | "color" | "points" | "rx" | "ry"
            | "fn" | "x.fn" | "y.fn" | "x.range" | "t.range" | "t.steps"
            | "x.scale" | "y.scale" | "steps" | "origin" | "dash"
            | "a" | "b" | "turns" | "sides" | "start"
            | "ctrl1" | "ctrl2" | "end" | "anchor" | "scale"
            | "polar" | "theta"
            => {}

            // ── BuildSection / BuildDiv / BuildPage specific ───────────────
            "route" | "auth" | "roles"
            | "meta.description" | "meta.keywords"
            | "onError" | "fallback" | "label" | "description" | "icon"
            => {}

            // ── Unknown attribute — warn ──────────────────────────────────
            other => {
                self.errors.push(TypeError::warn(
                    loc,
                    format!("Unknown attribute '{}' — will be passed through as data-bliss-{}", other, other.replace('.', "-"))
                ));
            }
        }
    }

    // ── Tailwind class checker ────────────────────────────────────────────

    fn check_tailwind_classes(&mut self, classes: &str, loc: &str) {
        for class in classes.split_whitespace() {
            // Strip responsive/state prefix for lookup
            let base = if let Some(idx) = class.rfind(':') {
                &class[idx + 1..]
            } else {
                class
            };

            // Skip if known
            if tailwind_lookup(base).is_some() { continue; }

            // Skip arbitrary value classes: bg-[#123], text-[2rem], etc.
            if base.contains('[') && base.contains(']') { continue; }

            // Skip CSS variable references
            if base.starts_with("var(") { continue; }

            // Skip negation utilities
            let stripped = base.strip_prefix('-').unwrap_or(base);
            if tailwind_lookup(stripped).is_some() { continue; }

            // Suggest corrections for common typos
            let suggestion = suggest_tailwind(base);
            let msg = if let Some(s) = suggestion {
                format!("Tailwind class '{}' not recognised — did you mean '{}'?", class, s)
            } else {
                format!("Tailwind class '{}' not recognised — check spelling or add to custom classes", class)
            };

            self.errors.push(TypeError::warn(loc, msg));
        }
    }

    // ── Geo canvas checker ────────────────────────────────────────────────

    fn check_geo_canvas(&mut self, attrs: &AttrList, children: &[GeoChild], loc: &str) {
        let has_width  = attrs.get_num("width").is_some();
        let has_height = attrs.get_num("height").is_some();

        if !has_width {
            self.errors.push(TypeError::error(loc, "DrawCanvas requires width attribute"));
        }
        if !has_height {
            self.errors.push(TypeError::error(loc, "DrawCanvas requires height attribute"));
        }

        for child in children {
            match child {
                GeoChild::Shape { kind, attrs: shape_attrs } => {
                    self.check_geo_shape(kind, shape_attrs, loc);
                }
                _ => {}
            }
        }
    }

    fn check_geo_shape(&mut self, kind: &str, attrs: &AttrList, loc: &str) {
        let shape_loc = format!("{}[{}]", loc, kind);
        match kind {
            "circle" => {
                if attrs.get_str("center").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "circle requires center attribute (e.g. center: \"200 200\")"));
                }
                if attrs.get_num("radius").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "circle requires radius attribute"));
                }
            }
            "rect" => {
                if attrs.get_str("at").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "rect requires at attribute (e.g. at: \"50 50\")"));
                }
            }
            "line" => {
                if attrs.get_str("from").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "line requires from attribute"));
                }
                if attrs.get_str("to").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "line requires to attribute"));
                }
            }
            "plot" => {
                if attrs.get_str("fn").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "plot requires fn attribute (e.g. fn: \"sin(x)\")"));
                }
            }
            "parametric" => {
                if attrs.get_str("x.fn").is_none() || attrs.get_str("y.fn").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "parametric requires x.fn and y.fn attributes"));
                }
            }
            "regularPolygon" => {
                if attrs.get_num("sides").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "regularPolygon requires sides attribute"));
                }
                if attrs.get_num("radius").is_none() {
                    self.errors.push(TypeError::error(&shape_loc, "regularPolygon requires radius attribute"));
                }
            }
            "polygon" | "ellipse" | "text" | "bezier" | "spiral" | "polar" => {}
            other => {
                self.errors.push(TypeError::warn(
                    &shape_loc,
                    format!("Unknown geo shape '{}' — will compile if supported by renderer", other)
                ));
            }
        }
    }
}

// ─── HTML5 tag validator ──────────────────────────────────────────────────────

fn is_valid_html5_tag(tag: &str) -> bool {
    matches!(tag,
        "a"|"abbr"|"address"|"area"|"article"|"aside"|"audio"|
        "b"|"base"|"bdi"|"bdo"|"blockquote"|"body"|"br"|"button"|
        "canvas"|"caption"|"cite"|"code"|"col"|"colgroup"|
        "data"|"datalist"|"dd"|"del"|"details"|"dfn"|"dialog"|"div"|"dl"|"dt"|
        "em"|"embed"|
        "fieldset"|"figcaption"|"figure"|"footer"|"form"|
        "h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"head"|"header"|"hgroup"|"hr"|"html"|
        "i"|"iframe"|"img"|"input"|"ins"|
        "kbd"|
        "label"|"legend"|"li"|"link"|
        "main"|"map"|"mark"|"menu"|"meta"|"meter"|
        "nav"|"noscript"|
        "object"|"ol"|"optgroup"|"option"|"output"|
        "p"|"picture"|"pre"|"progress"|
        "q"|
        "rp"|"rt"|"ruby"|
        "s"|"samp"|"script"|"section"|"select"|"small"|"source"|"span"|
        "strong"|"style"|"sub"|"summary"|"sup"|
        "table"|"tbody"|"td"|"template"|"textarea"|"tfoot"|"th"|"thead"|
        "time"|"title"|"tr"|"track"|
        "u"|"ul"|
        "var"|"video"|
        "wbr"
    )
}

// ─── Tailwind typo suggester ──────────────────────────────────────────────────

fn suggest_tailwind(class: &str) -> Option<&'static str> {
    let suggestions: &[(&str, &str)] = &[
        ("bg-grey",     "bg-gray"),
        ("colour",      "color"),
        ("centre",      "center"),
        ("grey",        "gray"),
        ("font-regular","font-normal"),
        ("flex-center", "items-center justify-center"),
        ("w-screen-full","w-full"),
        ("text-medium", "text-base"),
        ("border-round","rounded"),
        ("p-auto",      "mx-auto"),
        ("text-large",  "text-lg"),
        ("text-small",  "text-sm"),
        ("bg-primary",  "bg-blue-600"),
        ("bg-danger",   "bg-red-500"),
        ("bg-success",  "bg-green-500"),
        ("text-muted",  "text-slate-500"),
        ("d-flex",      "flex"),
        ("d-none",      "hidden"),
        ("d-block",     "block"),
        ("mt-auto",     "mt-auto"),
        ("px-auto",     "mx-auto"),
    ];

    for (typo, correct) in suggestions {
        if class.contains(typo) { return Some(correct); }
    }
    None
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Run the type checker on a collection of parsed files.
/// Returns (errors, warnings) separated for cleaner reporting.
pub fn check_project(
    files:    &[BlissFile],
    sections: &[String],
    divs:     &[String],
) -> (Vec<TypeError>, Vec<TypeError>) {
    let mut checker = TypeChecker::new(sections.to_vec(), divs.to_vec());

    for file in files {
        checker.check_file(file);
    }

    let all = checker.into_errors();
    let errors   = all.iter().filter(|e| e.severity == Severity::Error)  .cloned().collect();
    let warnings = all.iter().filter(|e| e.severity == Severity::Warning).cloned().collect();
    (errors, warnings)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_section(name: &str, children: Vec<Child>) -> BlissFile {
        BlissFile::Section(SectionNode {
            name:      name.to_string(),
            attrs:     vec![],
            props:     vec![],
            lifecycle: LifecycleHooks::default(),
            children,
        })
    }

    fn el(tag: &str, attrs: Vec<Attr>) -> Child {
        Child::Element(ElementNode { tag: tag.to_string(), attrs, children: vec![] })
    }

    fn attr_str(key: &str, val: &str) -> Attr {
        Attr {
            key:   key.split('.').map(str::to_string).collect(),
            value: AttrValue::Str(val.to_string()),
        }
    }

    fn attr_bool(key: &str, val: bool) -> Attr {
        Attr {
            key:   key.split('.').map(str::to_string).collect(),
            value: AttrValue::Bool(val),
        }
    }

    #[test]
    fn test_valid_section_passes() {
        let file = make_section("Hero", vec![
            el("h1", vec![
                attr_str("text", "Hello"),
                attr_str("style.tailwind", "text-white font-bold"),
                attr_str("animate", "fadeInUp"),
                attr_str("animate.trigger", "scroll"),
                attr_str("animate.delay", "200ms"),
            ])
        ]);
        let (errors, warnings) = check_project(&[file], &[], &[]);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_invalid_html_tag_errors() {
        let file = make_section("Test", vec![
            el("diiv", vec![]) // typo
        ]);
        let (errors, _) = check_project(&[file], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("diiv")));
    }

    #[test]
    fn test_invalid_animate_trigger_errors() {
        let file = make_section("Test", vec![
            el("h1", vec![
                attr_str("animate.trigger", "mouse") // invalid
            ])
        ]);
        let (errors, _) = check_project(&[file], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("animate.trigger")));
    }

    #[test]
    fn test_invalid_css_time_value_errors() {
        let file = make_section("Test", vec![
            el("h1", vec![
                attr_str("animate.delay", "300") // missing ms or s
            ])
        ]);
        let (errors, _) = check_project(&[file], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("CSS time value")));
    }

    #[test]
    fn test_valid_animate_delay_passes() {
        let file = make_section("Test", vec![
            el("h1", vec![attr_str("animate.delay", "300ms")])
        ]);
        let (errors, _) = check_project(&[file], &[], &[]);
        assert!(!errors.iter().any(|e| e.message.contains("animate.delay")));
    }

    #[test]
    fn test_missing_section_ref_errors() {
        let page = BlissFile::Page(PageNode {
            name:     "Landing".to_string(),
            attrs:    vec![],
            route:    Some("/".to_string()),
            output:   crate::compiler::ast::OutputMode::Static,
            layout:   None,
            is_layout: false,
            sections: vec![
                PageChild::Include {
                    name:  "NonExistent".to_string(),
                    attrs: vec![],
                }
            ],
        });
        let (errors, _) = check_project(&[page], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("NonExistent")));
    }

    #[test]
    fn test_known_section_ref_passes() {
        let page = BlissFile::Page(PageNode {
            name:     "Landing".to_string(),
            attrs:    vec![],
            route:    Some("/".to_string()),
            output:   crate::compiler::ast::OutputMode::Static,
            layout:   None,
            is_layout: false,
            sections: vec![
                PageChild::Include {
                    name:  "Hero".to_string(),
                    attrs: vec![],
                }
            ],
        });
        let (errors, _) = check_project(
            &[page],
            &["Hero".to_string()],
            &[],
        );
        assert!(!errors.iter().any(|e| e.message.contains("Hero")));
    }

    #[test]
    fn test_pwa_offline_page_requires_static_output_errors() {
        let page = BlissFile::Page(PageNode {
            name:     "Offline".to_string(),
            attrs:    vec![attr_bool("pwa.offline", true)],
            route:    Some("/offline".to_string()),
            output:   crate::compiler::ast::OutputMode::Runtime,
            layout:   None,
            is_layout: false,
            sections: vec![],
        });
        let (errors, _) = check_project(&[page], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("pwa.offline")));
    }

    #[test]
    fn test_pwa_offline_page_with_static_output_passes() {
        let page = BlissFile::Page(PageNode {
            name:     "Offline".to_string(),
            attrs:    vec![attr_bool("pwa.offline", true)],
            route:    Some("/offline".to_string()),
            output:   crate::compiler::ast::OutputMode::Static,
            layout:   None,
            is_layout: false,
            sections: vec![],
        });
        let (errors, _) = check_project(&[page], &[], &[]);
        assert!(!errors.iter().any(|e| e.message.contains("pwa.offline")));
    }

    #[test]
    fn test_tailwind_typo_warns() {
        let file = make_section("Test", vec![
            el("div", vec![attr_str("style.tailwind", "bg-bloo-500")])
        ]);
        let (_, warnings) = check_project(&[file], &[], &[]);
        assert!(warnings.iter().any(|w| w.message.contains("bg-bloo-500")));
    }

    #[test]
    fn test_valid_tailwind_passes() {
        let file = make_section("Test", vec![
            el("div", vec![attr_str("style.tailwind", "flex items-center text-white bg-slate-900 p-4")])
        ]);
        let (_, warnings) = check_project(&[file], &[], &[]);
        assert!(!warnings.iter().any(|w| w.message.contains("not recognised")));
    }

    #[test]
    fn test_javascript_uri_blocked() {
        let file = make_section("Test", vec![
            el("a", vec![attr_str("href", "javascript:alert(1)")])
        ]);
        let (errors, _) = check_project(&[file], &[], &[]);
        assert!(errors.iter().any(|e| e.message.contains("javascript:")));
    }
}
