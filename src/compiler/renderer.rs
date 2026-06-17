/// BlissLang Renderer
///
/// Walks the AST and produces HTML + CSS + JS output.
/// This is the first working renderer — it handles static output mode.
/// Runtime mode (Rust server templates) will be added in v0.2.

use crate::compiler::ast::*;
use std::collections::HashMap;

// ─── Render Config ────────────────────────────────────────────────────────────

pub struct RenderConfig {
    pub tailwind_cdn:  bool,
    pub animation_css: bool,
    pub title:         String,
    pub lang:          String,
    pub charset:       String,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            tailwind_cdn:  true,
            animation_css: true,
            title:         "BlissLang App".to_string(),
            lang:          "en".to_string(),
            charset:       "UTF-8".to_string(),
        }
    }
}

// ─── Render Context ───────────────────────────────────────────────────────────

pub struct RenderContext<'a> {
    pub sections:   &'a HashMap<String, SectionNode>,
    pub divs:       &'a HashMap<String, DivNode>,
    pub config:     &'a RenderConfig,
    /// Props passed into this render scope (from IncludeSection attrs)
    pub props:      HashMap<String, String>,
    pub indent:     usize,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        sections: &'a HashMap<String, SectionNode>,
        divs:     &'a HashMap<String, DivNode>,
        config:   &'a RenderConfig,
    ) -> Self {
        Self { sections, divs, config, props: HashMap::new(), indent: 0 }
    }

    fn indented(&self, s: &str) -> String {
        let pad = "  ".repeat(self.indent);
        format!("{}{}", pad, s)
    }

    fn child_ctx(&self) -> RenderContext<'a> {
        RenderContext {
            sections: self.sections,
            divs:     self.divs,
            config:   self.config,
            props:    self.props.clone(),
            indent:   self.indent + 1,
        }
    }
}

// ─── Renderer ─────────────────────────────────────────────────────────────────

pub struct Renderer;

impl Renderer {
    /// Render a complete page to an HTML string.
    pub fn render_page(
        page:     &PageNode,
        sections: &HashMap<String, SectionNode>,
        divs:     &HashMap<String, DivNode>,
        config:   &RenderConfig,
    ) -> String {
        let ctx = RenderContext::new(sections, divs, config);
        let mut out = String::new();

        // HTML boilerplate
        out.push_str("<!DOCTYPE html>\n");
        out.push_str(&format!("<html lang=\"{}\">\n", ctx.config.lang));
        out.push_str("<head>\n");
        out.push_str(&format!(
            "  <meta charset=\"{}\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
            ctx.config.charset
        ));
        out.push_str(&format!("  <title>{}</title>\n", ctx.config.title));

        // Tailwind via CDN (dev mode — will be replaced by compiled CSS in build mode)
        if ctx.config.tailwind_cdn {
            out.push_str("  <script src=\"https://cdn.tailwindcss.com\"></script>\n");
        }

        // BlissLang animation CSS
        if ctx.config.animation_css {
            out.push_str("  <style>\n");
            out.push_str(Self::animation_css());
            out.push_str("  </style>\n");
        }

        out.push_str("</head>\n<body>\n");

        // Render each section
        for child in &page.sections {
            match child {
                PageChild::Include { name, attrs } => {
                    if let Some(section) = sections.get(name) {
                        let mut section_ctx = ctx.child_ctx();
                        // Pass attrs as props
                        for attr in attrs {
                            if let AttrValue::Str(s) = &attr.value {
                                section_ctx.props.insert(attr.key_str(), s.clone());
                            }
                        }
                        out.push_str(&Self::render_section(section, &section_ctx));
                    } else {
                        out.push_str(&format!(
                            "<!-- WARNING: Section '{}' not found -->\n", name
                        ));
                    }
                }
                PageChild::Comment(c) => {
                    out.push_str(&format!("<!-- {} -->\n", c));
                }
                PageChild::ForEach { .. } => {
                    out.push_str("<!-- ForEach requires runtime mode -->\n");
                }
            }
        }

        out.push_str("  <script src=\"/_bliss/runtime.js\"></script>\n");
        out.push_str("</body>\n</html>\n");
        out
    }

    /// Render a section to an HTML string.
    pub fn render_section(section: &SectionNode, ctx: &RenderContext) -> String {
        let mut out = String::new();
        out.push_str(&format!("<!-- Section: {} -->\n", section.name));

        // Get section-level classes from attrs
        let tw = section.attrs.get_str("style.tailwind").unwrap_or("");
        let css = section.attrs.get_str("style.css").unwrap_or("");

        let mut class_attr = tw.to_string();
        let mut style_attr = css.to_string();

        let id = section.attrs.get_str("id").unwrap_or("").to_string();
        let id_str = if id.is_empty() { String::new() } else { format!(" id=\"{}\"", id) };
        let class_str = if class_attr.is_empty() { String::new() } else { format!(" class=\"{}\"", class_attr) };
        let style_str = if style_attr.is_empty() { String::new() } else { format!(" style=\"{}\"", style_attr) };

        out.push_str(&format!("<section{}{}{} data-bliss-section=\"{}\">\n",
            id_str, class_str, style_str, section.name));

        let child_ctx = ctx.child_ctx();
        for child in &section.children {
            out.push_str(&Self::render_child(child, &child_ctx));
        }

        out.push_str("</section>\n");
        out
    }

    /// Render a child node.
    fn render_child(child: &Child, ctx: &RenderContext) -> String {
        match child {
            Child::Element(el) => Self::render_element(el, ctx),
            Child::UseDiv { name, attrs, children } => Self::render_use_div(name, attrs, children, ctx),
            Child::ForEach { collection, binding, body, .. } => {
                Self::render_foreach(collection, binding, body, ctx)
            }
            Child::ShowIf { cond, then, else_ } => {
                Self::render_showif(cond, then, else_, ctx)
            }
            Child::GeoCanvas { attrs, children } => Self::render_geo_canvas(attrs, children, ctx),
            Child::Comment(c) => format!("<!-- {} -->\n", c),
            Child::Slot { name } => format!("<!-- slot:{} -->\n", name),
            Child::Responsive { breakpoint, body } => {
                Self::render_responsive(breakpoint, body, ctx)
            }
            Child::ErrorBoundary { fallback, body, .. } => {
                let mut out = format!("<div data-bliss-boundary data-fallback=\"{}\">\n", fallback);
                let child_ctx = ctx.child_ctx();
                for c in body { out.push_str(&Self::render_child(c, &child_ctx)); }
                out.push_str("</div>\n");
                out
            }
            // Real-time and event handlers are emitted as JS — skipped in static render
            Child::OnWS { channel_event, binding, .. } => {
                format!("<!-- OnWS: {} as {} -->\n", channel_event, binding)
            }
            Child::OnSSE { channel_event, binding, .. } => {
                format!("<!-- OnSSE: {} as {} -->\n", channel_event, binding)
            }
            Child::OnBridge { event, binding, .. } => {
                format!("<!-- OnBridge: {} as {} -->\n", event, binding)
            }
            Child::OnEvent { event, binding, .. } => {
                format!("<!-- OnEvent: {} as {} -->\n", event, binding)
            }
            Child::Into { slot, children } => {
                let mut out = format!("<div data-slot-content=\"{}\">\n", slot);
                let child_ctx = ctx.child_ctx();
                for c in children { out.push_str(&Self::render_child(c, &child_ctx)); }
                out.push_str("</div>\n");
                out
            }
            Child::Stmt(_) => String::new(), // statements don't produce HTML
            Child::UsePackage { name, .. } => {
                format!("<!-- Package: {} -->\n", name)
            }
        }
    }

    /// Render an HTML element.
    fn render_element(el: &ElementNode, ctx: &RenderContext) -> String {
        let pad = "  ".repeat(ctx.indent);
        let mut attrs_str = String::new();
        let mut text_content = None;
        let mut animate_class = String::new();
        let mut data_animate = String::new();

        for attr in &el.attrs {
            let key = attr.key_str();
            let val = Self::resolve_attr_value(&attr.value, ctx);

            match key.as_str() {
                "text" => { text_content = Some(val); }
                "style.tailwind" => {
                    attrs_str.push_str(&format!(" class=\"{}\"", val));
                }
                "style.css" => {
                    attrs_str.push_str(&format!(" style=\"{}\"", val));
                }
                "animate" => {
                    data_animate.push_str(&format!(" data-animate=\"{}\"", val));
                    animate_class.push_str(&format!("bliss-animate-{}", val));
                }
                "animate.delay" => {
                    data_animate.push_str(&format!(" data-animate-delay=\"{}\"", val));
                }
                "animate.duration" => {
                    data_animate.push_str(&format!(" data-animate-duration=\"{}\"", val));
                }
                "animate.trigger" => {
                    data_animate.push_str(&format!(" data-animate-trigger=\"{}\"", val));
                }
                "animate.threshold" => {
                    data_animate.push_str(&format!(" data-animate-threshold=\"{}\"", val));
                }
                "link" => {
                    // Shorthand: link="url" on a button wraps it or sets href
                    attrs_str.push_str(&format!(" onclick=\"window.location='{}';\"", val));
                }
                "reactive" => {
                    // Mark as reactive — JS runtime will handle updates
                    attrs_str.push_str(&format!(" data-reactive=\"{}\"", val));
                }
                "show" => {
                    attrs_str.push_str(&format!(" data-show=\"{}\"", val));
                }
                // Pass through all valid HTML attributes
                _ if Self::is_html_attr(&key) => {
                    let html_key = key.replace('.', "-");
                    attrs_str.push_str(&format!(" {}=\"{}\"", html_key, val));
                }
                // data.* attributes
                _ if key.starts_with("data.") => {
                    let data_key = key.replacen("data.", "data-", 1).replace('.', "-");
                    attrs_str.push_str(&format!(" {}=\"{}\"", data_key, val));
                }
                // Unknown attributes become data attributes (safe fallback)
                _ => {
                    attrs_str.push_str(&format!(" data-bliss-{}=\"{}\"", key.replace('.', "-"), val));
                }
            }
        }

        // Add animation class if present
        if !animate_class.is_empty() {
            attrs_str.push_str(&format!(" data-bliss-animate=\"{}\"", animate_class));
        }
        attrs_str.push_str(&data_animate);

        let is_void = Self::is_void_element(&el.tag);

        if is_void {
            return format!("{}<{}{} />\n", pad, el.tag, attrs_str);
        }

        let mut out = format!("{}<{}{}>\n", pad, el.tag, attrs_str);

        // Text content
        if let Some(text) = text_content {
            out.push_str(&format!("{}  {}\n", pad, Self::escape_html(&text)));
        }

        // Children
        let child_ctx = ctx.child_ctx();
        for child in &el.children {
            out.push_str(&Self::render_child(child, &child_ctx));
        }

        out.push_str(&format!("{}</{}>\n", pad, el.tag));
        out
    }

    /// Render UseDiv — looks up the div definition and renders it with supplied props.
    fn render_use_div(
        name:     &str,
        attrs:    &AttrList,
        children: &[Child],
        ctx:      &RenderContext,
    ) -> String {
        if let Some(div) = ctx.divs.get(name) {
            let mut div_ctx = ctx.child_ctx();
            // Pass attrs as props into the div's context
            for attr in attrs {
                if let AttrValue::Str(s) = &attr.value {
                    div_ctx.props.insert(attr.key_str(), s.clone());
                }
            }

            let mut out = format!("<!-- Div: {} -->\n", name);
            for child in &div.children {
                // Handle slots — inject the Into[] children from the caller
                if let Child::Slot { name: slot_name } = child {
                    // Find matching Into[] in caller's children
                    let slot_children: Vec<&Child> = children.iter()
                        .filter(|c| {
                            if let Child::Into { slot, .. } = c { slot == slot_name } else { false }
                        })
                        .flat_map(|c| {
                            if let Child::Into { children, .. } = c { children.iter() } else { [].iter() }
                        })
                        .collect();

                    for sc in slot_children {
                        out.push_str(&Self::render_child(sc, &div_ctx));
                    }
                } else {
                    out.push_str(&Self::render_child(child, &div_ctx));
                }
            }
            out
        } else {
            format!("<!-- WARNING: Div '{}' not found -->\n", name)
        }
    }

    /// Render ForEach — in static mode, renders a placeholder comment.
    /// Runtime mode will loop over actual data.
    fn render_foreach(
        collection: &str,
        binding:    &str,
        body:       &[Child],
        ctx:        &RenderContext,
    ) -> String {
        format!(
            "<!-- ForEach: {} as {} — {} items (runtime) -->\n",
            collection, binding, body.len()
        )
    }

    /// Render ShowIf — in static mode, renders both branches with data attributes
    /// so the JS runtime can toggle visibility.
    fn render_showif(
        cond:  &str,
        then:  &[Child],
        else_: &[Child],
        ctx:   &RenderContext,
    ) -> String {
        let mut out = String::new();
        out.push_str(&format!("<div data-showif=\"{}\">\n", Self::escape_attr(cond)));
        let child_ctx = ctx.child_ctx();
        for c in then { out.push_str(&Self::render_child(c, &child_ctx)); }
        out.push_str("</div>\n");

        if !else_.is_empty() {
            out.push_str(&format!("<div data-showelse=\"{}\">\n", Self::escape_attr(cond)));
            for c in else_ { out.push_str(&Self::render_child(c, &child_ctx)); }
            out.push_str("</div>\n");
        }

        out
    }

    /// Render responsive blocks.
    fn render_responsive(breakpoint: &Breakpoint, body: &[Child], ctx: &RenderContext) -> String {
        let bp_class = match breakpoint {
            Breakpoint::Mobile  => "bliss-mobile-only",
            Breakpoint::Tablet  => "bliss-tablet-only",
            Breakpoint::Desktop => "bliss-desktop-only",
        };
        let mut out = format!("<div class=\"{}\">\n", bp_class);
        let child_ctx = ctx.child_ctx();
        for c in body { out.push_str(&Self::render_child(c, &child_ctx)); }
        out.push_str("</div>\n");
        out
    }

    /// Render BlissGeo canvas to SVG.
    fn render_geo_canvas(attrs: &AttrList, children: &[GeoChild], ctx: &RenderContext) -> String {
        let width  = attrs.get_num("width").unwrap_or(400.0);
        let height = attrs.get_num("height").unwrap_or(300.0);
        let id     = attrs.get_str("id").unwrap_or("bliss-canvas");

        let mut out = format!(
            "<svg id=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">\n",
            id, width, height, width, height
        );

        for child in children {
            out.push_str(&Self::render_geo_child(child));
        }

        out.push_str("</svg>\n");
        out
    }

    fn render_geo_child(child: &GeoChild) -> String {
        match child {
            GeoChild::Shape { kind, attrs } => Self::render_geo_shape(kind, attrs),
            GeoChild::Comment(c) => format!("<!-- {} -->\n", c),
            GeoChild::VarDecl { name, .. } => format!("<!-- var {} -->\n", name),
            GeoChild::Repeat { binding, body, .. } => {
                // Static mode — render all body shapes (simplified)
                body.iter().map(|c| Self::render_geo_child(c)).collect::<String>()
            }
        }
    }

    fn render_geo_shape(kind: &str, attrs: &AttrList) -> String {
        match kind {
            "circle" => {
                let (cx, cy) = Self::parse_center(attrs.get_str("center").unwrap_or("0 0"));
                let r    = attrs.get_num("radius").unwrap_or(50.0);
                let fill = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw   = attrs.get_num("border.width").unwrap_or(1.0);
                format!("  <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />\n",
                    cx, cy, r, fill, stroke, sw)
            }
            "rect" => {
                let (x, y) = Self::parse_center(attrs.get_str("at").unwrap_or("0 0"));
                let w  = attrs.get_num("width").unwrap_or(100.0);
                let h  = attrs.get_num("height").unwrap_or(60.0);
                let rx = attrs.get_num("radius").unwrap_or(0.0);
                let fill = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw = attrs.get_num("border.width").unwrap_or(1.0);
                format!("  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />\n",
                    x, y, w, h, rx, fill, stroke, sw)
            }
            "line" => {
                let (x1, y1) = Self::parse_center(attrs.get_str("from").unwrap_or("0 0"));
                let (x2, y2) = Self::parse_center(attrs.get_str("to").unwrap_or("100 100"));
                let color = attrs.get_str("color").unwrap_or("#000");
                let w = attrs.get_num("width").unwrap_or(1.0);
                let dash = attrs.get_str("dash").unwrap_or("");
                let dash_str = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{}\"", dash) };
                format!("  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}  />\n",
                    x1, y1, x2, y2, color, w, dash_str)
            }
            "polygon" => {
                let pts = attrs.get_str("points").unwrap_or("0 0");
                let fill = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw = attrs.get_num("border.width").unwrap_or(1.0);
                format!("  <polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />\n",
                    pts, fill, stroke, sw)
            }
            "ellipse" => {
                let (cx, cy) = Self::parse_center(attrs.get_str("center").unwrap_or("0 0"));
                let rx = attrs.get_num("rx").unwrap_or(50.0);
                let ry = attrs.get_num("ry").unwrap_or(30.0);
                let fill = attrs.get_str("fill").unwrap_or("none");
                format!("  <ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" />\n",
                    cx, cy, rx, ry, fill)
            }
            "text" => {
                let (x, y) = Self::parse_center(attrs.get_str("at").unwrap_or("0 0"));
                let content = attrs.get_str("text").unwrap_or("");
                let fill = attrs.get_str("fill").unwrap_or("#000");
                let size = attrs.get_num("size").unwrap_or(16.0);
                format!("  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\">{}</text>\n",
                    x, y, fill, size, Self::escape_html(content))
            }
            other => {
                format!("  <!-- Unknown geo shape: {} -->\n", other)
            }
        }
    }

    // ── Built-in Animation CSS ────────────────────────────────────────────

    fn animation_css() -> &'static str {
        r#"
    /* BlissLang built-in animations */
    [data-animate] { opacity: 0; }
    [data-animate].bliss-visible { animation-fill-mode: both; }

    @keyframes bliss-fadeIn       { from { opacity: 0; } to { opacity: 1; } }
    @keyframes bliss-fadeInUp     { from { opacity: 0; transform: translateY(20px); } to { opacity: 1; transform: translateY(0); } }
    @keyframes bliss-fadeInDown   { from { opacity: 0; transform: translateY(-20px); } to { opacity: 1; transform: translateY(0); } }
    @keyframes bliss-fadeInLeft   { from { opacity: 0; transform: translateX(-20px); } to { opacity: 1; transform: translateX(0); } }
    @keyframes bliss-fadeInRight  { from { opacity: 0; transform: translateX(20px); } to { opacity: 1; transform: translateX(0); } }
    @keyframes bliss-zoomIn       { from { opacity: 0; transform: scale(0.9); } to { opacity: 1; transform: scale(1); } }
    @keyframes bliss-slideInLeft  { from { transform: translateX(-100%); } to { transform: translateX(0); } }
    @keyframes bliss-slideInRight { from { transform: translateX(100%); } to { transform: translateX(0); } }
    @keyframes bliss-slideInUp    { from { transform: translateY(100%); } to { transform: translateY(0); } }
    @keyframes bliss-bounceIn     { 0% { transform: scale(0.3); opacity: 0; } 50% { transform: scale(1.05); } 70% { transform: scale(0.9); } 100% { transform: scale(1); opacity: 1; } }
    @keyframes bliss-pulse        { 0%, 100% { transform: scale(1); } 50% { transform: scale(1.05); } }
    @keyframes bliss-shake        { 0%,100%{transform:translateX(0)} 20%{transform:translateX(-8px)} 40%{transform:translateX(8px)} 60%{transform:translateX(-8px)} 80%{transform:translateX(8px)} }
    @keyframes bliss-bounce       { 0%,100%{transform:translateY(0)} 50%{transform:translateY(-10px)} }
    @keyframes bliss-spin         { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

    .bliss-mobile-only  { display: block; }
    .bliss-tablet-only  { display: none; }
    .bliss-desktop-only { display: none; }

    @media (min-width: 768px) {
        .bliss-mobile-only  { display: none; }
        .bliss-tablet-only  { display: block; }
    }
    @media (min-width: 1024px) {
        .bliss-tablet-only  { display: none; }
        .bliss-desktop-only { display: block; }
    }
"#
    }

    // ── Minimal runtime JS ────────────────────────────────────────────────

    /// Returns the minimal JS that powers animations and ShowIf in static mode.
    pub fn runtime_js() -> &'static str {
        r#"
// BlissLang minimal runtime — animations and ShowIf
(function() {
    // Scroll-triggered animations via IntersectionObserver
    const animEls = document.querySelectorAll('[data-animate]');
    if (animEls.length && 'IntersectionObserver' in window) {
        const obs = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    const el    = entry.target;
                    const name  = el.dataset.animate;
                    const delay = el.dataset.animateDelay || '0ms';
                    const dur   = el.dataset.animateDuration || '600ms';
                    const trigger = el.dataset.animateTrigger || 'scroll';
                    if (trigger === 'scroll' || trigger === 'load') {
                        el.style.animationName     = `bliss-${name}`;
                        el.style.animationDuration = dur;
                        el.style.animationDelay    = delay;
                        el.style.animationFillMode = 'both';
                        el.style.opacity           = '';
                        el.classList.add('bliss-visible');
                        obs.unobserve(el);
                    }
                }
            });
        }, { threshold: 0.15 });
        animEls.forEach(el => obs.observe(el));
    }
})();
"#
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    fn resolve_attr_value(val: &AttrValue, ctx: &RenderContext) -> String {
        match val {
            AttrValue::Str(s)  => s.clone(),
            AttrValue::Number(n) => n.to_string(),
            AttrValue::Bool(b) => b.to_string(),
            AttrValue::Null    => String::new(),
            AttrValue::Expr(e) => {
                // Try resolving from props context
                ctx.props.get(e.as_str()).cloned().unwrap_or_else(|| format!("{{{}}}", e))
            }
            AttrValue::Interpolated(parts) => {
                parts.iter().map(|p| match p {
                    InterpolationPart::Literal(s) => s.clone(),
                    InterpolationPart::Expr(e) => {
                        ctx.props.get(e.as_str()).cloned().unwrap_or_else(|| format!("{{{}}}", e))
                    }
                }).collect()
            }
            AttrValue::Array(items) => {
                items.iter().map(|i| Self::resolve_attr_value(i, ctx)).collect::<Vec<_>>().join(", ")
            }
        }
    }

    fn parse_center(s: &str) -> (f64, f64) {
        let parts: Vec<f64> = s.split_whitespace()
            .filter_map(|p| p.parse().ok())
            .collect();
        (parts.get(0).copied().unwrap_or(0.0), parts.get(1).copied().unwrap_or(0.0))
    }

    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
         .replace('<', "&lt;")
         .replace('>', "&gt;")
         .replace('"', "&quot;")
    }

    fn escape_attr(s: &str) -> String {
        s.replace('"', "&quot;")
    }

    fn is_void_element(tag: &str) -> bool {
        matches!(tag, "area"|"base"|"br"|"col"|"embed"|"hr"|"img"|"input"|
                      "link"|"meta"|"param"|"source"|"track"|"wbr")
    }

    fn is_html_attr(key: &str) -> bool {
        matches!(key,
            "id"|"class"|"style"|"title"|"lang"|"dir"|"hidden"|"tabindex"|
            "draggable"|"contenteditable"|"spellcheck"|"translate"|
            "accesskey"|"href"|"src"|"alt"|"width"|"height"|"type"|
            "name"|"value"|"placeholder"|"required"|"readonly"|"disabled"|
            "checked"|"selected"|"multiple"|"maxlength"|"minlength"|
            "min"|"max"|"step"|"pattern"|"autocomplete"|"autofocus"|
            "action"|"method"|"enctype"|"novalidate"|"for"|"target"|
            "rel"|"download"|"hreflang"|"media"|"crossorigin"|"integrity"|
            "async"|"defer"|"charset"|"content"|"http-equiv"|"property"|
            "autoplay"|"controls"|"loop"|"muted"|"preload"|"poster"|
            "srcdoc"|"sandbox"|"allow"|"allowfullscreen"|"frameborder"|
            "colspan"|"rowspan"|"scope"|"headers"|"rows"|"cols"|"wrap"|
            "onclick"|"onchange"|"oninput"|"onsubmit"|"onkeydown"|
            "onkeyup"|"onkeypress"|"onfocus"|"onblur"|"onmouseover"|
            "onmouseout"|"onmouseenter"|"onmouseleave"|"onload"|"onerror"|
            "role"|"aria-label"|"aria-hidden"|"aria-expanded"|"aria-controls"|
            "aria-describedby"|"aria-labelledby"|"aria-live"|"aria-atomic"|
            "loading"|"decoding"|"referrerpolicy"|"fetchpriority"
        )
    }
}
