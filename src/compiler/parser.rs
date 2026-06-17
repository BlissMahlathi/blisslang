/// BlissLang Parser
///
/// Converts the flat token stream from the lexer into a typed AST.
/// The parser is a hand-written recursive descent parser — one function
/// per grammar rule, matching the formal grammar in the specification.

use crate::compiler::lexer::{Token, TokenKind};
use crate::compiler::ast::*;
use thiserror::Error;

// ─── Parse Error ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unexpected token '{got}' at line {line}, col {col} — expected {expected}")]
    Unexpected { expected: String, got: String, line: usize, col: usize },

    #[error("Unexpected end of file — expected {expected}")]
    UnexpectedEof { expected: String },

    #[error("Empty file — no top-level declaration found")]
    EmptyFile,

    #[error("Unknown top-level keyword '{keyword}' at line {line}")]
    UnknownTopLevel { keyword: String, line: usize },

    #[error("Props block can only appear once per section/div at line {line}")]
    DuplicateProps { line: usize },
}

type ParseResult<T> = Result<T, ParseError>;

// ─── Parser ───────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens:  Vec<Token>,
    pos:     usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        // Filter out comments before parsing
        let tokens = tokens
            .into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Comment(_)))
            .collect();
        Self { tokens, pos: 0 }
    }

    // ── Public entry ──────────────────────────────────────────────────────

    pub fn parse(mut self) -> ParseResult<BlissFile> {
        self.skip_newlines();

        if self.at_end() {
            return Err(ParseError::EmptyFile);
        }

        let file = match &self.current().kind {
            TokenKind::BuildPage      => BlissFile::Page(self.parse_page()?),
            TokenKind::BuildSection   => BlissFile::Section(self.parse_section()?),
            TokenKind::BuildDiv       => BlissFile::Div(self.parse_div()?),
            TokenKind::BuildArticle   => BlissFile::Article(self.parse_article()?),
            TokenKind::CreateState    => BlissFile::State(self.parse_state()?),
            TokenKind::CreateModel    => BlissFile::Model(self.parse_model()?),
            TokenKind::DefineAnimation=> BlissFile::Animation(self.parse_animation()?),
            TokenKind::DefineType     => BlissFile::TypeDef(self.parse_typedef()?),
            TokenKind::ApiRoute       => BlissFile::ApiRoute(self.parse_api_route()?),
            TokenKind::Ident(s)       => {
                let kw = s.clone();
                let line = self.current().line;
                return Err(ParseError::UnknownTopLevel { keyword: kw, line });
            }
            _ => {
                let tok = self.current();
                return Err(ParseError::Unexpected {
                    expected: "top-level declaration".into(),
                    got:      format!("{}", tok.kind),
                    line:     tok.line,
                    col:      tok.col,
                });
            }
        };

        Ok(file)
    }

    // ── Page ──────────────────────────────────────────────────────────────

    fn parse_page(&mut self) -> ParseResult<PageNode> {
        self.expect(TokenKind::BuildPage)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Page").to_string();
        let route = attrs.get_str("route").map(str::to_string);
        let output = attrs.get_str("output").map(str::to_string);

        let mut sections = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) || self.at_end() { break; }

            match &self.current().kind {
                TokenKind::IncludeSection => {
                    self.advance();
                    let sec_name = self.parse_bracketed_string()?;
                    let sec_attrs = if self.check(TokenKind::LBracket) {
                        self.parse_attr_list()?
                    } else { vec![] };
                    self.skip_newlines();
                    sections.push(PageChild::Include { name: sec_name, attrs: sec_attrs });
                }
                TokenKind::Comment(c) => {
                    let c = c.clone();
                    self.advance();
                    sections.push(PageChild::Comment(c));
                }
                _ => { self.advance(); }
            }
        }

        self.expect(TokenKind::Dedent)?;

        Ok(PageNode { name, attrs, route, output, sections })
    }

    // ── Section ───────────────────────────────────────────────────────────

    fn parse_section(&mut self) -> ParseResult<SectionNode> {
        self.expect(TokenKind::BuildSection)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Section").to_string();

        let mut props     = Vec::new();
        let mut lifecycle = LifecycleHooks::default();
        let mut children  = Vec::new();
        let mut has_props = false;

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) || self.at_end() { break; }

            match &self.current().kind {
                TokenKind::Props => {
                    if has_props {
                        return Err(ParseError::DuplicateProps { line: self.current().line });
                    }
                    has_props = true;
                    props = self.parse_props_block()?;
                }
                TokenKind::OnInit    => { lifecycle.on_init    = self.parse_lifecycle_hook()?; }
                TokenKind::OnMount   => { lifecycle.on_mount   = self.parse_lifecycle_hook()?; }
                TokenKind::OnDestroy => { lifecycle.on_destroy = self.parse_lifecycle_hook()?; }
                TokenKind::OnUpdate  => {
                    self.advance();
                    self.expect(TokenKind::LBracket)?;
                    let prev = self.expect_ident()?;
                    self.expect(TokenKind::Comma)?;
                    let next = self.expect_ident()?;
                    self.expect(TokenKind::RBracket)?;
                    self.expect(TokenKind::Colon)?;
                    self.expect_newline()?;
                    self.expect(TokenKind::Indent)?;
                    let body = self.parse_stmt_block()?;
                    self.expect(TokenKind::Dedent)?;
                    lifecycle.on_update = Some((prev, next, body));
                }
                _ => {
                    if let Some(child) = self.parse_child()? {
                        children.push(child);
                    }
                }
            }
        }

        self.expect(TokenKind::Dedent)?;

        Ok(SectionNode { name, attrs, props, lifecycle, children })
    }

    // ── Div ───────────────────────────────────────────────────────────────

    fn parse_div(&mut self) -> ParseResult<DivNode> {
        self.expect(TokenKind::BuildDiv)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Div").to_string();

        let mut props     = Vec::new();
        let mut lifecycle = LifecycleHooks::default();
        let mut children  = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) || self.at_end() { break; }

            match &self.current().kind {
                TokenKind::Props     => { props = self.parse_props_block()?; }
                TokenKind::OnMount   => { lifecycle.on_mount   = self.parse_lifecycle_hook()?; }
                TokenKind::OnDestroy => { lifecycle.on_destroy = self.parse_lifecycle_hook()?; }
                _ => {
                    if let Some(child) = self.parse_child()? {
                        children.push(child);
                    }
                }
            }
        }

        self.expect(TokenKind::Dedent)?;

        Ok(DivNode { name, attrs, props, lifecycle, children })
    }

    // ── Article ───────────────────────────────────────────────────────────

    fn parse_article(&mut self) -> ParseResult<ArticleNode> {
        self.expect(TokenKind::BuildArticle)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Article").to_string();
        let mut props    = Vec::new();
        let mut children = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) || self.at_end() { break; }

            match &self.current().kind {
                TokenKind::Props => { props = self.parse_props_block()?; }
                _ => {
                    if let Some(child) = self.parse_child()? {
                        children.push(child);
                    }
                }
            }
        }

        self.expect(TokenKind::Dedent)?;
        Ok(ArticleNode { name, attrs, props, children })
    }

    // ── Props Block ───────────────────────────────────────────────────────

    fn parse_props_block(&mut self) -> ParseResult<Vec<PropDef>> {
        self.expect(TokenKind::Props)?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let mut props = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }

            let name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.expect_ident()?;

            let mut required = false;
            let mut default  = None;

            // Parse optional modifiers: required | default: value
            while self.check(TokenKind::Comma) {
                self.advance();
                let modifier = self.expect_ident()?;
                match modifier.as_str() {
                    "required" => { required = true; }
                    "default"  => {
                        self.expect(TokenKind::Colon)?;
                        default = Some(self.parse_attr_value()?);
                    }
                    _ => {}
                }
            }

            self.skip_newlines();
            props.push(PropDef { name, ty, required, default });
        }

        self.expect(TokenKind::Dedent)?;
        Ok(props)
    }

    // ── Lifecycle Hook ────────────────────────────────────────────────────

    fn parse_lifecycle_hook(&mut self) -> ParseResult<Vec<Stmt>> {
        self.advance(); // consume OnInit/OnMount/OnDestroy
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let stmts = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;
        Ok(stmts)
    }

    // ── Child Nodes ───────────────────────────────────────────────────────

    fn parse_child(&mut self) -> ParseResult<Option<Child>> {
        self.skip_newlines();
        if self.check(TokenKind::Dedent) || self.at_end() {
            return Ok(None);
        }

        let child = match &self.current().kind.clone() {
            TokenKind::UseDiv => self.parse_use_div()?,
            TokenKind::UsePackage => self.parse_use_package()?,
            TokenKind::ForEach => self.parse_foreach()?,
            TokenKind::ShowIf | TokenKind::Show => self.parse_showif()?,
            TokenKind::OnMobile  => { self.advance(); Child::Responsive { breakpoint: Breakpoint::Mobile,  body: self.parse_responsive_body()? } }
            TokenKind::OnTablet  => { self.advance(); Child::Responsive { breakpoint: Breakpoint::Tablet,  body: self.parse_responsive_body()? } }
            TokenKind::OnDesktop => { self.advance(); Child::Responsive { breakpoint: Breakpoint::Desktop, body: self.parse_responsive_body()? } }
            TokenKind::Slot => self.parse_slot()?,
            TokenKind::Into => self.parse_into()?,
            TokenKind::OnWS => self.parse_on_ws()?,
            TokenKind::OnSSE => self.parse_on_sse()?,
            TokenKind::OnBridge => self.parse_on_bridge()?,
            TokenKind::OnEvent => self.parse_on_event()?,
            TokenKind::DrawCanvas => self.parse_geo_canvas()?,
            TokenKind::ErrorBoundary => self.parse_error_boundary()?,
            TokenKind::Comment(c) => {
                let c = c.clone();
                self.advance();
                Child::Comment(c)
            }
            // Anything else that looks like an identifier is an HTML element
            TokenKind::Ident(_) => self.parse_element()?,
            // Skip stray newlines
            TokenKind::Newline => {
                self.advance();
                return Ok(None);
            }
            _ => {
                // Try to parse as a statement
                let stmt = self.parse_stmt()?;
                Child::Stmt(stmt)
            }
        };

        Ok(Some(child))
    }

    // ── Element ───────────────────────────────────────────────────────────

    fn parse_element(&mut self) -> ParseResult<Child> {
        let tag = self.expect_ident()?;
        let attrs = if self.check(TokenKind::LBracket) {
            self.parse_attr_list()?
        } else { vec![] };

        // Optional body  element[...]:  NEWLINE INDENT children DEDENT
        let children = if self.check(TokenKind::Colon) {
            self.advance();
            self.expect_newline()?;
            self.expect(TokenKind::Indent)?;
            let mut kids = Vec::new();
            while !self.check(TokenKind::Dedent) && !self.at_end() {
                if let Some(c) = self.parse_child()? {
                    kids.push(c);
                }
            }
            self.expect(TokenKind::Dedent)?;
            kids
        } else {
            self.skip_newlines();
            vec![]
        };

        Ok(Child::Element(ElementNode { tag, attrs, children }))
    }

    // ── UseDiv ────────────────────────────────────────────────────────────

    fn parse_use_div(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::UseDiv)?;
        let name = self.parse_bracketed_string()?;
        let attrs = if self.check(TokenKind::LBracket) {
            self.parse_attr_list()?
        } else { vec![] };

        let children = if self.check(TokenKind::Colon) {
            self.advance();
            self.expect_newline()?;
            self.expect(TokenKind::Indent)?;
            let mut kids = Vec::new();
            while !self.check(TokenKind::Dedent) && !self.at_end() {
                if let Some(c) = self.parse_child()? { kids.push(c); }
            }
            self.expect(TokenKind::Dedent)?;
            kids
        } else {
            self.skip_newlines();
            vec![]
        };

        Ok(Child::UseDiv { name, attrs, children })
    }

    // ── UsePackage ────────────────────────────────────────────────────────

    fn parse_use_package(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::UsePackage)?;
        let name = self.parse_bracketed_string()?;
        let attrs = if self.check(TokenKind::LBracket) {
            self.parse_attr_list()?
        } else { vec![] };
        self.skip_newlines();
        Ok(Child::UsePackage { name, attrs })
    }

    // ── ForEach ───────────────────────────────────────────────────────────

    fn parse_foreach(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::ForEach)?;
        let collection = self.parse_bracketed_string()?;
        self.expect(TokenKind::As)?;
        let binding = self.expect_ident()?;

        let track = if self.check(TokenKind::Track) {
            self.advance();
            Some(self.parse_bracketed_string()?)
        } else { None };

        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let mut body = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            if let Some(c) = self.parse_child()? { body.push(c); }
        }
        self.expect(TokenKind::Dedent)?;

        Ok(Child::ForEach { collection, binding, track, body })
    }

    // ── ShowIf ────────────────────────────────────────────────────────────

    fn parse_showif(&mut self) -> ParseResult<Child> {
        self.advance(); // ShowIf or Show
        let cond = self.parse_bracketed_string()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let mut then = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            if let Some(c) = self.parse_child()? { then.push(c); }
        }
        self.expect(TokenKind::Dedent)?;

        let else_ = if self.check(TokenKind::ShowElse) {
            self.advance();
            self.expect(TokenKind::Colon)?;
            self.expect_newline()?;
            self.expect(TokenKind::Indent)?;
            let mut kids = Vec::new();
            while !self.check(TokenKind::Dedent) && !self.at_end() {
                if let Some(c) = self.parse_child()? { kids.push(c); }
            }
            self.expect(TokenKind::Dedent)?;
            kids
        } else { vec![] };

        Ok(Child::ShowIf { cond, then, else_ })
    }

    // ── Responsive ────────────────────────────────────────────────────────

    fn parse_responsive_body(&mut self) -> ParseResult<Vec<Child>> {
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let mut body = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            if let Some(c) = self.parse_child()? { body.push(c); }
        }
        self.expect(TokenKind::Dedent)?;
        Ok(body)
    }

    // ── Slot / Into ───────────────────────────────────────────────────────

    fn parse_slot(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::Slot)?;
        let attrs = self.parse_attr_list()?;
        let name = attrs.get_str("name").unwrap_or("default").to_string();
        self.skip_newlines();
        Ok(Child::Slot { name })
    }

    fn parse_into(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::Into)?;
        let attrs = self.parse_attr_list()?;
        let slot = attrs.get_str("slot").unwrap_or("default").to_string();
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let mut children = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            if let Some(c) = self.parse_child()? { children.push(c); }
        }
        self.expect(TokenKind::Dedent)?;
        Ok(Child::Into { slot, children })
    }

    // ── Real-time hooks ───────────────────────────────────────────────────

    fn parse_on_ws(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::OnWS)?;
        let channel_event = self.parse_bracketed_string()?;
        self.expect(TokenKind::As)?;
        let binding = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let body = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;
        Ok(Child::OnWS { channel_event, binding, body })
    }

    fn parse_on_sse(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::OnSSE)?;
        let channel_event = self.parse_bracketed_string()?;
        self.expect(TokenKind::As)?;
        let binding = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let body = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;
        Ok(Child::OnSSE { channel_event, binding, body })
    }

    fn parse_on_bridge(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::OnBridge)?;
        let event = self.parse_bracketed_string()?;
        self.expect(TokenKind::As)?;
        let binding = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let body = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;
        Ok(Child::OnBridge { event, binding, body })
    }

    fn parse_on_event(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::OnEvent)?;
        let event = self.parse_bracketed_string()?;
        self.expect(TokenKind::As)?;
        let binding = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let body = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;
        Ok(Child::OnEvent { event, binding, body })
    }

    // ── Error Boundary ────────────────────────────────────────────────────

    fn parse_error_boundary(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::ErrorBoundary)?;
        let attrs = self.parse_attr_list()?;
        let fallback = attrs.get_str("fallback").unwrap_or("Error").to_string();
        let on_error = attrs.get_str("onError").map(str::to_string);
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;
        let mut body = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            if let Some(c) = self.parse_child()? { body.push(c); }
        }
        self.expect(TokenKind::Dedent)?;
        Ok(Child::ErrorBoundary { fallback, on_error, body })
    }

    // ── BlissGeo Canvas ───────────────────────────────────────────────────

    fn parse_geo_canvas(&mut self) -> ParseResult<Child> {
        self.expect(TokenKind::DrawCanvas)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let mut children = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }
            match &self.current().kind.clone() {
                TokenKind::Var => {
                    self.advance();
                    let name = self.expect_ident()?;
                    self.expect(TokenKind::Equals)?;
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    children.push(GeoChild::VarDecl { name, value });
                }
                TokenKind::Repeat => {
                    self.advance();
                    self.expect(TokenKind::LBracket)?;
                    let binding = self.expect_ident()?;
                    self.expect(TokenKind::From)?;
                    let from = self.parse_expr()?;
                    self.expect(TokenKind::To)?;
                    let to = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    self.expect(TokenKind::Colon)?;
                    self.expect_newline()?;
                    self.expect(TokenKind::Indent)?;
                    let mut body = Vec::new();
                    while !self.check(TokenKind::Dedent) && !self.at_end() {
                        self.skip_newlines();
                        if self.check(TokenKind::Dedent) { break; }
                        let kind = self.expect_ident()?;
                        let a = self.parse_attr_list()?;
                        self.skip_newlines();
                        body.push(GeoChild::Shape { kind, attrs: a });
                    }
                    self.expect(TokenKind::Dedent)?;
                    children.push(GeoChild::Repeat { binding, from, to, body });
                }
                TokenKind::Comment(c) => {
                    let c = c.clone(); self.advance();
                    children.push(GeoChild::Comment(c));
                }
                _ => {
                    let kind = self.expect_ident()?;
                    let geo_attrs = self.parse_attr_list()?;
                    self.skip_newlines();
                    children.push(GeoChild::Shape { kind, attrs: geo_attrs });
                }
            }
        }
        self.expect(TokenKind::Dedent)?;
        Ok(Child::GeoCanvas { attrs, children })
    }

    // ── State ─────────────────────────────────────────────────────────────

    fn parse_state(&mut self) -> ParseResult<StateNode> {
        self.expect(TokenKind::CreateState)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("State").to_string();
        let mut signals = Vec::new();
        let mut derived = Vec::new();
        let mut effects = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }

            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;

            // Check what follows: Signal[...], Derived[...], Effect[...], or plain value
            match &self.current().kind.clone() {
                TokenKind::Ident(ty) if ty == "Signal" => {
                    self.advance();
                    let inner_attrs = self.parse_attr_list()?;
                    let ty_str = inner_attrs.get_str("type").unwrap_or("Any").to_string();
                    let default = inner_attrs.get_attr("default").cloned()
                        .unwrap_or(AttrValue::Null);
                    self.skip_newlines();
                    signals.push(SignalDef { name: field_name, ty: ty_str, default });
                }
                TokenKind::Ident(ty) if ty == "Derived" => {
                    self.advance();
                    let inner_attrs = self.parse_attr_list()?;
                    let from    = inner_attrs.get_str("from").unwrap_or("").to_string();
                    let compute = inner_attrs.get_str("compute").unwrap_or("").to_string();
                    self.skip_newlines();
                    derived.push(DerivedDef { name: field_name, from, compute });
                }
                TokenKind::Ident(ty) if ty == "Effect" => {
                    self.advance();
                    let inner_attrs = self.parse_attr_list()?;
                    let watch = inner_attrs.get_str("watch").unwrap_or("").to_string();
                    self.expect(TokenKind::Colon)?;
                    self.expect_newline()?;
                    self.expect(TokenKind::Indent)?;
                    // Parse when clauses
                    let mut body = Vec::new();
                    while !self.check(TokenKind::Dedent) && !self.at_end() {
                        self.skip_newlines();
                        if self.check(TokenKind::Dedent) { break; }
                        if self.check(TokenKind::When) {
                            self.advance();
                            let cond = self.parse_attr_value()?;
                            self.expect(TokenKind::Colon)?;
                            let stmt = self.parse_stmt()?;
                            body.push(WhenClause { cond, body: vec![stmt] });
                        } else {
                            self.advance();
                        }
                    }
                    self.expect(TokenKind::Dedent)?;
                    effects.push(EffectDef { watch, body });
                }
                _ => {
                    // plain default value — treat as a signal
                    let val = self.parse_attr_value()?;
                    self.skip_newlines();
                    signals.push(SignalDef {
                        name: field_name,
                        ty: "Any".into(),
                        default: val,
                    });
                }
            }
        }

        self.expect(TokenKind::Dedent)?;
        Ok(StateNode { name, signals, derived, effects })
    }

    // ── Model ─────────────────────────────────────────────────────────────

    fn parse_model(&mut self) -> ParseResult<ModelNode> {
        self.expect(TokenKind::CreateModel)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Model").to_string();
        let mut fields = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }

            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;

            let mut ty = self.expect_ident()?;
            let mut modifiers = Vec::new();

            // Collect comma-separated modifiers: primary, required, auto, unique
            while self.check(TokenKind::Comma) {
                self.advance();
                let m = self.expect_ident()?;
                modifiers.push(m);
            }

            self.skip_newlines();
            fields.push(ModelField { name: field_name, ty, modifiers });
        }

        self.expect(TokenKind::Dedent)?;
        Ok(ModelNode { name, fields })
    }

    // ── Animation ─────────────────────────────────────────────────────────

    fn parse_animation(&mut self) -> ParseResult<AnimationNode> {
        self.expect(TokenKind::DefineAnimation)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("anim").to_string();
        let mut frames = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }

            if self.current().kind == TokenKind::Ident("frame".to_string()) {
                self.advance();
                let frame_attrs = self.parse_attr_list()?;
                let at = frame_attrs.get_str("at").unwrap_or("0%").to_string();
                self.expect(TokenKind::Colon)?;
                self.expect_newline()?;
                self.expect(TokenKind::Indent)?;

                let mut props = std::collections::HashMap::new();
                while !self.check(TokenKind::Dedent) && !self.at_end() {
                    self.skip_newlines();
                    if self.check(TokenKind::Dedent) { break; }
                    let prop_key = self.expect_ident()?;
                    self.expect(TokenKind::Colon)?;
                    let prop_val = match self.parse_attr_value()? {
                        AttrValue::Str(s) => s,
                        AttrValue::Number(n) => n.to_string(),
                        other => format!("{:?}", other),
                    };
                    self.skip_newlines();
                    props.insert(prop_key, prop_val);
                }
                self.expect(TokenKind::Dedent)?;
                frames.push(AnimationFrame { at, props });
            } else {
                self.advance();
            }
        }

        self.expect(TokenKind::Dedent)?;
        Ok(AnimationNode { name, frames })
    }

    // ── TypeDef ───────────────────────────────────────────────────────────

    fn parse_typedef(&mut self) -> ParseResult<TypeDefNode> {
        self.expect(TokenKind::DefineType)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let name = attrs.get_str("name").unwrap_or("Type").to_string();
        let mut fields = Vec::new();

        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }

            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty_raw = self.expect_ident()?;
            let optional = ty_raw.starts_with("Optional");
            let ty = ty_raw.trim_start_matches("Optional[").trim_end_matches(']').to_string();
            self.skip_newlines();
            fields.push(TypeField { name: field_name, ty, optional });
        }

        self.expect(TokenKind::Dedent)?;
        Ok(TypeDefNode { name, fields })
    }

    // ── API Route ─────────────────────────────────────────────────────────

    fn parse_api_route(&mut self) -> ParseResult<ApiRouteNode> {
        self.expect(TokenKind::ApiRoute)?;
        let attrs = self.parse_attr_list()?;
        self.expect(TokenKind::Colon)?;
        self.expect_newline()?;
        self.expect(TokenKind::Indent)?;

        let path   = attrs.get_str("path").unwrap_or("/").to_string();
        let method = match attrs.get_str("method").unwrap_or("GET") {
            "POST"   => HttpMethod::Post,
            "PUT"    => HttpMethod::Put,
            "PATCH"  => HttpMethod::Patch,
            "DELETE" => HttpMethod::Delete,
            _        => HttpMethod::Get,
        };
        let auth = match attrs.get_str("auth").unwrap_or("none") {
            "required" => AuthRequirement::Required,
            "optional" => AuthRequirement::Optional,
            _          => AuthRequirement::None,
        };
        let roles = Vec::new(); // TODO: parse roles array

        let body = self.parse_stmt_block()?;
        self.expect(TokenKind::Dedent)?;

        Ok(ApiRouteNode { path, method, auth, roles, body })
    }

    // ── Statement Block ───────────────────────────────────────────────────

    fn parse_stmt_block(&mut self) -> ParseResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.check(TokenKind::Dedent) && !self.at_end() {
            self.skip_newlines();
            if self.check(TokenKind::Dedent) { break; }
            let s = self.parse_stmt()?;
            stmts.push(s);
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        // Simplified statement parser — covers the most common cases
        match &self.current().kind.clone() {
            TokenKind::Return => {
                self.advance();
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::Return(expr))
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::Await(expr))
            }
            TokenKind::Var => {
                self.advance();
                let name = self.expect_ident()?;
                self.expect(TokenKind::Equals)?;
                let value = self.parse_expr()?;
                self.skip_newlines();
                Ok(Stmt::VarDecl { name, value })
            }
            TokenKind::When => {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect(TokenKind::Colon)?;
                self.expect_newline()?;
                self.expect(TokenKind::Indent)?;
                let body = self.parse_stmt_block()?;
                self.expect(TokenKind::Dedent)?;
                Ok(Stmt::When { cond, body })
            }
            TokenKind::Try => {
                self.advance();
                self.expect(TokenKind::Colon)?;
                self.expect_newline()?;
                self.expect(TokenKind::Indent)?;
                let try_body = self.parse_stmt_block()?;
                self.expect(TokenKind::Dedent)?;

                let mut catches = Vec::new();
                while self.check(TokenKind::Catch) {
                    self.advance();
                    let error_type = if self.check(TokenKind::LBracket) {
                        self.advance();
                        let t = self.expect_ident()?;
                        self.expect(TokenKind::RBracket)?;
                        Some(t)
                    } else { None };
                    let binding = if self.check(TokenKind::As) {
                        self.advance();
                        Some(self.expect_ident()?)
                    } else { None };
                    self.expect(TokenKind::Colon)?;
                    self.expect_newline()?;
                    self.expect(TokenKind::Indent)?;
                    let body = self.parse_stmt_block()?;
                    self.expect(TokenKind::Dedent)?;
                    catches.push(CatchClause { error_type, binding, body });
                }

                let finally_body = if self.check(TokenKind::Finally) {
                    self.advance();
                    self.expect(TokenKind::Colon)?;
                    self.expect_newline()?;
                    self.expect(TokenKind::Indent)?;
                    let b = self.parse_stmt_block()?;
                    self.expect(TokenKind::Dedent)?;
                    b
                } else { vec![] };

                Ok(Stmt::TryCatch { try_body, catches, finally_body })
            }
            _ => {
                // Assignment or expression
                let expr = self.parse_expr()?;

                // Check for assignment operator
                if self.check(TokenKind::Equals) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    Ok(Stmt::Assign {
                        path: format!("{:?}", expr),
                        op: AssignOp::Set,
                        value,
                    })
                } else if self.check(TokenKind::PlusEq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    Ok(Stmt::Assign {
                        path: format!("{:?}", expr),
                        op: AssignOp::AddEq,
                        value,
                    })
                } else if self.check(TokenKind::MinusEq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    self.skip_newlines();
                    Ok(Stmt::Assign {
                        path: format!("{:?}", expr),
                        op: AssignOp::SubEq,
                        value,
                    })
                } else {
                    self.skip_newlines();
                    Ok(Stmt::Call(expr))
                }
            }
        }
    }

    // ── Expression Parser ─────────────────────────────────────────────────

    fn parse_expr(&mut self) -> ParseResult<Expr> {
        self.parse_binary_expr()
    }

    fn parse_binary_expr(&mut self) -> ParseResult<Expr> {
        let left = self.parse_unary_expr()?;

        let op = match &self.current().kind {
            TokenKind::EqEq    => Some(BinOp::Eq),
            TokenKind::NotEq   => Some(BinOp::NotEq),
            TokenKind::Lt      => Some(BinOp::Lt),
            TokenKind::Gt      => Some(BinOp::Gt),
            TokenKind::LtEq    => Some(BinOp::LtEq),
            TokenKind::GtEq    => Some(BinOp::GtEq),
            TokenKind::AndAnd  => Some(BinOp::And),
            TokenKind::OrOr    => Some(BinOp::Or),
            TokenKind::Plus    => Some(BinOp::Add),
            TokenKind::Minus   => Some(BinOp::Sub),
            TokenKind::Star    => Some(BinOp::Mul),
            TokenKind::Slash   => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            TokenKind::NullCoal=> Some(BinOp::NullCoal),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let right = self.parse_unary_expr()?;
            return Ok(Expr::Binary { left: Box::new(left), op, right: Box::new(right) });
        }

        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> ParseResult<Expr> {
        match &self.current().kind {
            TokenKind::Bang  => { self.advance(); let e = self.parse_primary()?; Ok(Expr::Unary { op: UnaryOp::Not, expr: Box::new(e) }) }
            TokenKind::Minus => { self.advance(); let e = self.parse_primary()?; Ok(Expr::Unary { op: UnaryOp::Neg, expr: Box::new(e) }) }
            _ => self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        match &self.current().kind.clone() {
            TokenKind::StringLit(s) => { let s = s.clone(); self.advance(); Ok(Expr::Str(s)) }
            TokenKind::NumberLit(n) => { let n = *n; self.advance(); Ok(Expr::Number(n)) }
            TokenKind::BoolLit(b)   => { let b = *b; self.advance(); Ok(Expr::Bool(b)) }
            TokenKind::Null         => { self.advance(); Ok(Expr::Null) }
            TokenKind::LParen       => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::Ident(_) => {
                let name = self.expect_ident()?;
                // dot-access path
                if self.check(TokenKind::Dot) || self.check(TokenKind::LParen) {
                    // For now just return as string path — full path resolver in v0.2
                    let mut path = name;
                    while self.check(TokenKind::Dot) {
                        self.advance();
                        let field = self.expect_ident()?;
                        path.push('.');
                        path.push_str(&field);
                    }
                    // function call
                    if self.check(TokenKind::LParen) {
                        self.advance();
                        let mut args = Vec::new();
                        while !self.check(TokenKind::RParen) && !self.at_end() {
                            args.push(self.parse_expr()?);
                            if self.check(TokenKind::Comma) { self.advance(); }
                        }
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::Call { callee: Box::new(Expr::Ident(path)), args });
                    }
                    Ok(Expr::Ident(path))
                } else if self.check(TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !self.check(TokenKind::RParen) && !self.at_end() {
                        args.push(self.parse_expr()?);
                        if self.check(TokenKind::Comma) { self.advance(); }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Expr::Call { callee: Box::new(Expr::Ident(name)), args })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => {
                let tok = self.current();
                Err(ParseError::Unexpected {
                    expected: "expression".into(),
                    got:      format!("{}", tok.kind),
                    line:     tok.line,
                    col:      tok.col,
                })
            }
        }
    }

    // ── Attribute List ────────────────────────────────────────────────────

    fn parse_attr_list(&mut self) -> ParseResult<AttrList> {
        self.expect(TokenKind::LBracket)?;
        let mut attrs = Vec::new();

        loop {
            // Skip whitespace tokens inside brackets (multiline attr lists)
            while matches!(self.current().kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                self.advance();
            }

            if self.check(TokenKind::RBracket) || self.at_end() { break; }

            // key (possibly dotted)
            let mut key = vec![self.expect_ident()?];
            while self.check(TokenKind::Dot) {
                self.advance();
                key.push(self.expect_ident()?);
            }

            self.expect(TokenKind::Colon)?;
            let value = self.parse_attr_value()?;

            attrs.push(Attr { key, value });

            // Skip trailing whitespace/newlines inside brackets
            while matches!(self.current().kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                self.advance();
            }

            if self.check(TokenKind::Comma) { self.advance(); }
        }

        self.expect(TokenKind::RBracket)?;
        Ok(attrs)
    }

    fn parse_attr_value(&mut self) -> ParseResult<AttrValue> {
        match &self.current().kind.clone() {
            TokenKind::StringLit(s) => { let s = s.clone(); self.advance(); Ok(AttrValue::Str(s)) }
            TokenKind::NumberLit(n) => { let n = *n; self.advance(); Ok(AttrValue::Number(n)) }
            TokenKind::BoolLit(b)   => { let b = *b; self.advance(); Ok(AttrValue::Bool(b)) }
            TokenKind::Null         => { self.advance(); Ok(AttrValue::Null) }
            TokenKind::LBracket     => {
                self.advance();
                let mut items = Vec::new();
                while !self.check(TokenKind::RBracket) && !self.at_end() {
                    items.push(self.parse_attr_value()?);
                    if self.check(TokenKind::Comma) { self.advance(); }
                }
                self.expect(TokenKind::RBracket)?;
                Ok(AttrValue::Array(items))
            }
            TokenKind::Ident(s) => {
                // A bare identifier in an attr value is an expression reference
                let s = s.clone();
                self.advance();
                Ok(AttrValue::Expr(s))
            }
            _ => {
                let tok = self.current();
                Err(ParseError::Unexpected {
                    expected: "attribute value".into(),
                    got:      format!("{}", tok.kind),
                    line:     tok.line,
                    col:      tok.col,
                })
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Parse ["SomeName"] — a bracketed string literal
    fn parse_bracketed_string(&mut self) -> ParseResult<String> {
        self.expect(TokenKind::LBracket)?;
        let s = match &self.current().kind.clone() {
            TokenKind::StringLit(s) => { let s = s.clone(); self.advance(); s }
            _ => {
                let tok = self.current();
                return Err(ParseError::Unexpected {
                    expected: "string literal".into(),
                    got:      format!("{}", tok.kind),
                    line:     tok.line,
                    col:      tok.col,
                });
            }
        };
        self.expect(TokenKind::RBracket)?;
        Ok(s)
    }

    fn expect_ident(&mut self) -> ParseResult<String> {
        match &self.current().kind.clone() {
            TokenKind::Ident(s) => { let s = s.clone(); self.advance(); Ok(s) }
            // Allow keywords to be used as identifiers in some positions
            other => {
                // Convert keyword tokens to their string name for use as identifiers
                let s = format!("{}", other);
                if s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    || s.chars().all(|c| c.is_alphabetic() || c == '_')
                {
                    self.advance();
                    Ok(s)
                } else {
                    let tok = self.current();
                    Err(ParseError::Unexpected {
                        expected: "identifier".into(),
                        got:      s,
                        line:     tok.line,
                        col:      tok.col,
                    })
                }
            }
        }
    }

    fn expect(&mut self, kind: TokenKind) -> ParseResult<()> {
        if self.current().kind == kind {
            self.advance();
            Ok(())
        } else {
            let tok = self.current();
            Err(ParseError::Unexpected {
                expected: format!("{}", kind),
                got:      format!("{}", tok.kind),
                line:     tok.line,
                col:      tok.col,
            })
        }
    }

    fn expect_newline(&mut self) -> ParseResult<()> {
        self.skip_newlines();
        Ok(())
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn at_end(&self) -> bool {
        matches!(self.tokens[self.pos].kind, TokenKind::Eof)
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenKind::Newline) {
            self.advance();
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

pub fn parse(tokens: Vec<Token>) -> ParseResult<BlissFile> {
    Parser::new(tokens).parse()
}
