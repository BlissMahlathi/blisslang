/// BlissLang Renderer
///
/// Walks the AST and produces HTML + CSS + JS output.
/// This is the first working renderer — it handles static output mode.
/// Runtime mode (Rust server templates) will be added in v0.2.

use crate::compiler::ast::*;
use std::collections::HashMap; // used in render_page

// ─── Render Config ────────────────────────────────────────────────────────────

pub struct RenderConfig {
    pub title:   String,
    pub lang:    String,
    pub charset: String,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            title:   "BlissLang App".to_string(),
            lang:    "en".to_string(),
            charset: "UTF-8".to_string(),
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

    #[allow(dead_code)]
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
        pages:    &HashMap<String, PageNode>,
        sections: &HashMap<String, SectionNode>,
        divs:     &HashMap<String, DivNode>,
        config:   &RenderConfig,
        states:   &HashMap<String, StateNode>,
    ) -> String {
        let ctx = RenderContext::new(sections, divs, config);

        // Render this page's own section list into a body fragment.
        // If this page itself has no layout, this IS the final body.
        // If it has a layout, this fragment gets injected into the layout's Slot["content"].
        let own_body = Self::render_page_sections(page, &ctx, None);

        // Resolve layout chain: walk up `layout:` references, composing each
        // layout's sections with the previous body injected at Slot["content"].
        let body_html = if let Some(layout_name) = &page.layout {
            Self::resolve_layout_chain(layout_name, own_body, pages, &ctx, 0)
        } else {
            own_body
        };

        // ── P0.5: Generate purged CSS from rendered body ──────────────────
        let purged_css = crate::compiler::style::build_purged_css(&[&body_html]);

        // ── P0.3: Generate state initialisation JS blocks ─────────────────
        let mut state_js = String::new();
        for (_, state_node) in states {
            state_js.push_str(&Self::render_state_init(state_node));
        }

        // ── Assemble full HTML ─────────────────────────────────────────────
        let mut out = String::new();
        out.push_str("<!DOCTYPE html>\n");
        out.push_str(&format!("<html lang=\"{}\">\n", ctx.config.lang));
        out.push_str("<head>\n");
        out.push_str(&format!(
            "  <meta charset=\"{}\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n",
            ctx.config.charset
        ));

        // Page title from attrs or config
        let title = page.attrs.get_str("title")
            .unwrap_or(&ctx.config.title)
            .to_string();
        out.push_str(&format!("  <title>{}</title>\n", Self::escape_html(&title)));

        // ── P0.5: Purged CSS instead of CDN ───────────────────────────────
        out.push_str("  <style>\n");
        out.push_str(&purged_css);
        out.push_str("  </style>\n");

        out.push_str("</head>\n<body>\n");
        out.push_str(&body_html);

        // ── P0.3: State init JS (before runtime.js) ───────────────────────
        if !state_js.is_empty() {
            out.push_str("<script>\n");
            out.push_str("// BlissLang State Initialisation\n");
            out.push_str("(function() {\n");
            out.push_str("  document.addEventListener('DOMContentLoaded', function() {\n");
            out.push_str("    if (!window.__bliss) return;\n");
            out.push_str(&state_js);
            out.push_str("  });\n");
            out.push_str("})();\n");
            out.push_str("</script>\n");
        }

        out.push_str("  <script src=\"/_bliss/runtime.js\"></script>\n");
        out.push_str("</body>\n</html>\n");
        out
    }

    /// Render a page's own `IncludeSection[]` / `Slot[]` list into an HTML fragment.
    /// `injected_content` is the rendered body of a child page, used when this
    /// page is itself being used as a layout — it fills the `Slot["content"]` spot.
    fn render_page_sections(
        page:             &PageNode,
        ctx:              &RenderContext,
        injected_content: Option<&str>,
    ) -> String {
        let mut body_html = String::new();

        for child in &page.sections {
            match child {
                PageChild::Include { name, attrs } => {
                    if let Some(section) = ctx.sections.get(name) {
                        let mut section_ctx = ctx.child_ctx();
                        for attr in attrs {
                            if let AttrValue::Str(s) = &attr.value {
                                section_ctx.props.insert(attr.key_str(), s.clone());
                            }
                        }
                        body_html.push_str(&Self::render_section(section, &section_ctx));
                    } else {
                        body_html.push_str(&format!("<!-- WARNING: Section '{}' not found -->\n", name));
                    }
                }
                PageChild::Slot { name } => {
                    // Layout's content slot — inject the child page's rendered body here.
                    // If no child content was provided (this page rendered standalone),
                    // leave a marker comment so it's obvious in raw output.
                    match injected_content {
                        Some(content) => body_html.push_str(content),
                        None => body_html.push_str(&format!("<!-- Slot[\"{}\"]: no content injected -->\n", name)),
                    }
                }
                PageChild::Comment(c) => {
                    body_html.push_str(&format!("<!-- {} -->\n", c));
                }
                PageChild::ForEach { collection, binding, section, .. } => {
                    body_html.push_str(&format!(
                        "<!-- ForEach: {} as {} → {} (runtime mode required for live data) -->\n",
                        collection, binding, section
                    ));
                }
            }
        }

        body_html
    }

    /// Walk the layout chain: a page can specify `layout: "MainLayout"`, and that
    /// layout page can itself specify another layout, nesting arbitrarily deep.
    /// `depth` guards against accidental circular layout references.
    fn resolve_layout_chain(
        layout_name: &str,
        child_body:  String,
        pages:       &HashMap<String, PageNode>,
        ctx:         &RenderContext,
        depth:       usize,
    ) -> String {
        if depth > 8 {
            return format!(
                "<!-- ERROR: layout chain too deep (possible circular reference at '{}') -->\n{}",
                layout_name, child_body
            );
        }

        // Layouts are looked up by page name, not by route
        let layout_page = pages.values().find(|p| p.name == layout_name);

        match layout_page {
            Some(layout) => {
                let this_level = Self::render_page_sections(layout, ctx, Some(&child_body));
                match &layout.layout {
                    Some(next_layout) => Self::resolve_layout_chain(next_layout, this_level, pages, ctx, depth + 1),
                    None => this_level,
                }
            }
            None => {
                format!(
                    "<!-- WARNING: layout \"{}\" not found -->\n{}",
                    layout_name, child_body
                )
            }
        }
    }

    /// Render a section to an HTML string.
    pub fn render_section(section: &SectionNode, ctx: &RenderContext) -> String {
        let mut out = String::new();
        out.push_str(&format!("<!-- Section: {} -->\n", section.name));

        // Get section-level classes from attrs
        let tw  = section.attrs.get_str("style.tailwind").unwrap_or("");
        let css = section.attrs.get_str("style.css").unwrap_or("");

        let class_attr = tw.to_string();
        let style_attr = css.to_string();

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

    /// P0.3 — Emit JS signal initialisation for a CreateState node.
    fn render_state_init(state: &StateNode) -> String {
        let mut js = format!("    // State: {}\n    var _s = {{}};\n", state.name);

        for sig in &state.signals {
            let default_js = Self::attr_value_to_js(&sig.default);
            js.push_str(&format!(
                "    _s.{} = window.__bliss.signal({});\n",
                sig.name, default_js
            ));
        }

        for derived in &state.derived {
            js.push_str(&format!(
                "    _s.{} = window.__bliss.derived(function() {{ var s = window.__bliss.state.{}; return {}; }});\n",
                derived.name, state.name, derived.compute
            ));
        }

        js.push_str(&format!(
            "    window.__bliss.state.{} = _s;\n\n",
            state.name
        ));
        js
    }

    fn attr_value_to_js(val: &AttrValue) -> String {
        match val {
            AttrValue::Str(s)    => format!("\"{}\"", s),
            AttrValue::Number(n) => n.to_string(),
            AttrValue::Bool(b)   => b.to_string(),
            AttrValue::Null      => "null".to_string(),
            AttrValue::Array(items) => {
                let items_js: Vec<String> = items.iter()
                    .map(Self::attr_value_to_js)
                    .collect();
                format!("[{}]", items_js.join(", "))
            }
            AttrValue::Expr(e) => e.clone(),
            AttrValue::Interpolated(_) => "null".to_string(),
        }
    }

    /// P0.4 — Render ForEach with a data-foreach container and item template.
    fn render_foreach(
        collection: &str,
        binding:    &str,
        body:       &[Child],
        _ctx:       &RenderContext,
    ) -> String {
        // In static mode: emit a container div with data-foreach attribute
        // and a hidden template item. The signal system will clone and populate
        // the template when the collection signal updates.
        let child_ctx = _ctx.child_ctx();
        let mut template_html = String::new();
        for child in body {
            template_html.push_str(&Self::render_child(child, &child_ctx));
        }

        format!(
            "<div data-foreach=\"{}\" data-binding=\"{}\">\n  <template data-foreach-item>\n{}</template>\n</div>\n",
            collection, binding, template_html
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
    fn render_geo_canvas(attrs: &AttrList, children: &[GeoChild], _ctx: &RenderContext) -> String {
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
            GeoChild::Repeat { body, .. } => {
                // Static mode — render all body shapes (simplified)
                body.iter().map(|c| Self::render_geo_child(c)).collect::<String>()
            }
        }
    }

    fn render_geo_shape(kind: &str, attrs: &AttrList) -> String {
        use crate::geo::plotter;
        match kind {
            "circle" => {
                let (cx, cy) = Self::parse_center(attrs.get_str("center").unwrap_or("0 0"));
                let r    = attrs.get_num("radius").unwrap_or(50.0);
                let fill = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw   = attrs.get_num("border.width").unwrap_or(1.0);
                let geo_anim = attrs.get_str("geo.animate").unwrap_or("");
                let geo_dur  = attrs.get_str("geo.duration").unwrap_or("2s");
                let geo_rep  = attrs.get_str("geo.repeat").unwrap_or("infinite");
                let anim_attrs = if geo_anim.is_empty() { String::new() }
                    else { format!(" data-geo-animate=\"{}\" data-geo-duration=\"{}\" data-geo-repeat=\"{}\"", geo_anim, geo_dur, geo_rep) };
                format!("  <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}  />\n",
                    cx, cy, r, fill, stroke, sw, anim_attrs)
            }
            "rect" => {
                let (x, y) = Self::parse_center(attrs.get_str("at").unwrap_or("0 0"));
                let w  = attrs.get_num("width").unwrap_or(100.0);
                let h  = attrs.get_num("height").unwrap_or(60.0);
                let rx = attrs.get_num("radius").unwrap_or(0.0);
                let fill   = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw     = attrs.get_num("border.width").unwrap_or(1.0);
                format!("  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />\n",
                    x, y, w, h, rx, fill, stroke, sw)
            }
            "line" => {
                let (x1, y1) = Self::parse_center(attrs.get_str("from").unwrap_or("0 0"));
                let (x2, y2) = Self::parse_center(attrs.get_str("to").unwrap_or("100 100"));
                let color = attrs.get_str("color").unwrap_or("#000");
                let w     = attrs.get_num("width").unwrap_or(1.0);
                let dash  = attrs.get_str("dash").unwrap_or("");
                let dash_str = if dash.is_empty() { String::new() }
                    else { format!(" stroke-dasharray=\"{}\"", dash) };
                format!("  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}  />\n",
                    x1, y1, x2, y2, color, w, dash_str)
            }
            "polygon" => {
                let pts    = attrs.get_str("points").unwrap_or("0 0");
                let fill   = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw     = attrs.get_num("border.width").unwrap_or(1.0);
                let geo_anim = attrs.get_str("geo.animate").unwrap_or("");
                let geo_dur  = attrs.get_str("geo.duration").unwrap_or("2s");
                let geo_rep  = attrs.get_str("geo.repeat").unwrap_or("infinite");
                let anim_attrs = if geo_anim.is_empty() { String::new() }
                    else { format!(" data-geo-animate=\"{}\" data-geo-duration=\"{}\" data-geo-repeat=\"{}\"", geo_anim, geo_dur, geo_rep) };
                format!("  <polygon points=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}  />\n",
                    pts, fill, stroke, sw, anim_attrs)
            }
            "ellipse" => {
                let (cx, cy) = Self::parse_center(attrs.get_str("center").unwrap_or("0 0"));
                let rx   = attrs.get_num("rx").unwrap_or(50.0);
                let ry   = attrs.get_num("ry").unwrap_or(30.0);
                let fill = attrs.get_str("fill").unwrap_or("none");
                let stroke = attrs.get_str("border.color").unwrap_or("none");
                let sw   = attrs.get_num("border.width").unwrap_or(1.0);
                format!("  <ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />\n",
                    cx, cy, rx, ry, fill, stroke, sw)
            }
            "text" => {
                let (x, y) = Self::parse_center(attrs.get_str("at").unwrap_or("0 0"));
                let content = attrs.get_str("text").unwrap_or("");
                let fill    = attrs.get_str("fill").unwrap_or("#000");
                let size    = attrs.get_num("size").unwrap_or(16.0);
                let anchor  = attrs.get_str("anchor").unwrap_or("start");
                format!("  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" text-anchor=\"{}\">{}</text>\n",
                    x, y, fill, size, anchor, Self::escape_html(content))
            }
            // ── Math-powered shapes (via geo plotter) ─────────────────────
            "plot"           => format!("{}\n", plotter::render_plot(attrs)),
            "parametric"     => format!("{}\n", plotter::render_parametric(attrs)),
            "polar"          => format!("{}\n", plotter::render_polar(attrs)),
            "spiral"         => format!("{}\n", plotter::render_spiral(attrs)),
            "bezier"         => format!("{}\n", plotter::render_bezier(attrs)),
            "regularPolygon" => format!("{}\n", plotter::render_regular_polygon(attrs)),
            other => format!("  <!-- Unknown geo shape: {} -->\n", other)
        }
    }

    // ── Built-in Animation CSS (superseded by style.rs purger in v0.4) ──────

    #[allow(dead_code)]
    fn animation_css() -> &'static str { "" }

    // ── Minimal runtime JS ────────────────────────────────────────────────

    /// Returns the minimal JS that powers animations and ShowIf in static mode.
    #[allow(dead_code)]
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
            AttrValue::Str(s)    => Self::interpolate_string(s, ctx),
            AttrValue::Number(n) => n.to_string(),
            AttrValue::Bool(b)   => b.to_string(),
            AttrValue::Null      => String::new(),
            AttrValue::Expr(e)   => {
                ctx.props.get(e.as_str()).cloned()
                    .unwrap_or_else(|| format!("{{{}}}", e))
            }
            AttrValue::Interpolated(parts) => {
                parts.iter().map(|p| match p {
                    InterpolationPart::Literal(s) => s.clone(),
                    InterpolationPart::Expr(e) => {
                        ctx.props.get(e.as_str()).cloned()
                            .unwrap_or_else(|| format!("{{{}}}", e))
                    }
                }).collect()
            }
            AttrValue::Array(items) => {
                items.iter()
                    .map(|i| Self::resolve_attr_value(i, ctx))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
    }

    /// Resolve {props.X} and {State.X} placeholders in a string value.
    /// Handles BlissLang interpolation syntax: text: "Hello {props.name}"
    fn interpolate_string(s: &str, ctx: &RenderContext) -> String {
        if !s.contains('{') {
            return s.to_string();
        }

        let mut result = String::new();
        let mut chars  = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Collect expression inside braces
                let mut expr = String::new();
                let mut depth = 1usize;
                for inner in chars.by_ref() {
                    match inner {
                        '{' => { depth += 1; expr.push(inner); }
                        '}' => {
                            depth -= 1;
                            if depth == 0 { break; }
                            expr.push(inner);
                        }
                        c => expr.push(c),
                    }
                }

                let expr = expr.trim();

                // Resolve props.X
                if let Some(prop_name) = expr.strip_prefix("props.") {
                    if let Some(val) = ctx.props.get(prop_name) {
                        result.push_str(val);
                        continue;
                    }
                }

                // Resolve State.X or App.X paths from props context
                if let Some(val) = ctx.props.get(expr) {
                    result.push_str(val);
                    continue;
                }

                // Unresolved — keep as placeholder for the JS signal system
                result.push_str(&format!("{{{{{}}}}}", expr));
            } else {
                result.push(ch);
            }
        }

        result
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
