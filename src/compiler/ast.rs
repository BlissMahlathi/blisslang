/// BlissLang Abstract Syntax Tree (AST)
///
/// Every BlissLang source file is parsed into one of these node variants.
/// The renderer then walks the AST to produce HTML/CSS/JS output.

use std::collections::HashMap;

// ─── Attribute Values ─────────────────────────────────────────────────────────

/// The value side of  key: value  in an attribute list.
#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    Str(String),
    Number(f64),
    Bool(bool),
    Null,
    /// A string with \{expr} interpolation parts
    Interpolated(Vec<InterpolationPart>),
    /// ["a", "b", "c"]
    Array(Vec<AttrValue>),
    /// Reference to a state/prop path: State.user.name
    Expr(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationPart {
    Literal(String),
    Expr(String),
}

/// A single attribute key: value pair.
/// Key can be dotted: style.tailwind, animate.delay, data.userid
#[derive(Debug, Clone)]
pub struct Attr {
    /// e.g. ["style", "tailwind"] or ["name"] or ["data", "userid"]
    pub key:   Vec<String>,
    pub value: AttrValue,
}

impl Attr {
    pub fn key_str(&self) -> String {
        self.key.join(".")
    }

    /// Get the value as a plain string if it is one
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            AttrValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

pub type AttrList = Vec<Attr>;

// ─── Expressions ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    Str(String),
    Number(f64),
    Bool(bool),
    Null,
    Ident(String),
    /// a.b.c or a[0].b
    Path(Vec<PathSegment>),
    /// fn(arg1, arg2)
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// left op right
    Binary { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    /// !expr or -expr
    Unary { op: UnaryOp, expr: Box<Expr> },
    /// cond ? then : else
    Ternary { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    /// String with \{} parts
    Interpolated(Vec<InterpolationPart>),
}

#[derive(Debug, Clone)]
pub enum PathSegment {
    Field(String),
    Index(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
    Add, Sub, Mul, Div, Mod,
    NullCoal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp { Not, Neg }

// ─── Statements ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    /// path = expr
    Assign { path: String, op: AssignOp, value: Expr },
    /// fn(args)
    Call(Expr),
    /// await fn(args)
    Await(Expr),
    /// when expr: body
    When { cond: Expr, body: Vec<Stmt> },
    /// Try: ... Catch[T] as e: ... Finally: ...
    TryCatch {
        try_body:     Vec<Stmt>,
        catches:      Vec<CatchClause>,
        finally_body: Vec<Stmt>,
    },
    /// return expr
    Return(Expr),
    /// var name = expr
    VarDecl { name: String, value: Expr },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp { Set, AddEq, SubEq }

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub error_type: Option<String>,
    pub binding:    Option<String>,
    pub body:       Vec<Stmt>,
}

// ─── Lifecycle Hooks ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LifecycleHooks {
    pub on_init:    Vec<Stmt>,
    pub on_mount:   Vec<Stmt>,
    /// (prev_name, next_name, body)
    pub on_update:  Option<(String, String, Vec<Stmt>)>,
    pub on_destroy: Vec<Stmt>,
}

impl Default for LifecycleHooks {
    fn default() -> Self {
        Self {
            on_init:    Vec::new(),
            on_mount:   Vec::new(),
            on_update:  None,
            on_destroy: Vec::new(),
        }
    }
}

// ─── Props Definition ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PropDef {
    pub name:     String,
    pub ty:       String,
    pub required: bool,
    pub default:  Option<AttrValue>,
}

// ─── Child Nodes ──────────────────────────────────────────────────────────────

/// Anything that can appear as a child inside a section, div, or element body.
#[derive(Debug, Clone)]
pub enum Child {
    /// A standard HTML5 element: h1[], div[], button[], etc.
    Element(ElementNode),
    /// UseDiv["Card"][...] or UseDiv["Card"][...]: body
    UseDiv { name: String, attrs: AttrList, children: Vec<Child> },
    /// UsePackage["stripe-elements"][...]
    UsePackage { name: String, attrs: AttrList },
    /// ForEach["State.items"] as item track "item.id": body
    ForEach { collection: String, binding: String, track: Option<String>, body: Vec<Child> },
    /// ShowIf["expr"]: body / ShowElse: body
    ShowIf { cond: String, then: Vec<Child>, else_: Vec<Child> },
    /// OnMobile/OnTablet/OnDesktop: body
    Responsive { breakpoint: Breakpoint, body: Vec<Child> },
    /// Slot[name: "body"]
    Slot { name: String },
    /// Into[slot: "body"]: children
    Into { slot: String, children: Vec<Child> },
    /// OnWS["channel:event"] as data: stmts
    OnWS { channel_event: String, binding: String, body: Vec<Stmt> },
    /// OnSSE["channel:event"] as data: stmts
    OnSSE { channel_event: String, binding: String, body: Vec<Stmt> },
    /// OnBridge["pkg:event"] as data: stmts
    OnBridge { event: String, binding: String, body: Vec<Stmt> },
    /// OnEvent["name"] as data: stmts
    OnEvent { event: String, binding: String, body: Vec<Stmt> },
    /// DrawCanvas[...]: geo_children
    GeoCanvas { attrs: AttrList, children: Vec<GeoChild> },
    /// ErrorBoundary[fallback: "Card", onError: "log"]: body
    ErrorBoundary { fallback: String, on_error: Option<String>, body: Vec<Child> },
    /// BuildForm[name: "ContactForm", action: "/api/contact"]: Field[...] SubmitButton[...]
    Form(FormNode),
    /// A raw statement inside a section body (assignments, calls)
    Stmt(Stmt),
    /// # comment
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Breakpoint { Mobile, Tablet, Desktop }

// ─── Forms (spec Part 12: BuildForm / Field / SubmitButton) ────────────────

/// A single field-level validation rule, parsed from a `validator: "..."`
/// attribute. Multiple rules are pipe-separated: `"required|email"`.
#[derive(Debug, Clone, PartialEq)]
pub enum Validator {
    Required,
    Email,
    Number,
    Url,
    MinLength(u32),
    MaxLength(u32),
    Min(f64),
    Max(f64),
    /// Regex pattern the value must match.
    Pattern(String),
    /// Value must equal the named sibling field (e.g. confirm-password).
    Match(String),
    /// South African 13-digit ID number (DOB range + Luhn checksum).
    ZaId,
    /// South African mobile/landline number (+27… or 0…).
    ZaPhone,
}

impl Validator {
    /// Parses one `|`-separated validator token, e.g. `"min_length:8"`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Some(rest) = s.strip_prefix("min_length:") {
            return rest.trim().parse().ok().map(Validator::MinLength);
        }
        if let Some(rest) = s.strip_prefix("max_length:") {
            return rest.trim().parse().ok().map(Validator::MaxLength);
        }
        if let Some(rest) = s.strip_prefix("min:") {
            return rest.trim().parse().ok().map(Validator::Min);
        }
        if let Some(rest) = s.strip_prefix("max:") {
            return rest.trim().parse().ok().map(Validator::Max);
        }
        if let Some(rest) = s.strip_prefix("pattern:") {
            return Some(Validator::Pattern(rest.trim().to_string()));
        }
        if let Some(rest) = s.strip_prefix("match:") {
            return Some(Validator::Match(rest.trim().to_string()));
        }
        match s {
            "required"  => Some(Validator::Required),
            "email"     => Some(Validator::Email),
            "number"    => Some(Validator::Number),
            "url"       => Some(Validator::Url),
            "za_id"     => Some(Validator::ZaId),
            "za_phone"  => Some(Validator::ZaPhone),
            _ => None,
        }
    }

    /// Client-side `data-validators` token this rule serializes to.
    pub fn to_token(&self) -> String {
        match self {
            Validator::Required       => "required".to_string(),
            Validator::Email          => "email".to_string(),
            Validator::Number         => "number".to_string(),
            Validator::Url            => "url".to_string(),
            Validator::MinLength(n)   => format!("min_length:{}", n),
            Validator::MaxLength(n)   => format!("max_length:{}", n),
            Validator::Min(n)         => format!("min:{}", n),
            Validator::Max(n)         => format!("max:{}", n),
            Validator::Pattern(p)     => format!("pattern:{}", p),
            Validator::Match(f)       => format!("match:{}", f),
            Validator::ZaId           => "za_id".to_string(),
            Validator::ZaPhone        => "za_phone".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FormField {
    pub name:          String,
    /// text | email | password | tel | number | url | date | textarea |
    /// select | checkbox | radio | file
    pub field_type:    String,
    pub label:         Option<String>,
    pub placeholder:   Option<String>,
    pub required:      bool,
    pub validators:    Vec<Validator>,
    /// (value, label) pairs for select/radio
    pub options:       Vec<(String, String)>,
    /// accepted MIME/extension list for type: "file"
    pub accept:        Option<String>,
    /// human-readable max size for type: "file", e.g. "5MB" (display only —
    /// enforced client-side against parsed bytes)
    pub max_size:      Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FormSubmit {
    pub text:         String,
    pub loading_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FormNode {
    pub name:   String,
    pub action: Option<String>,
    pub method: String,
    pub fields: Vec<FormField>,
    pub submit: Option<FormSubmit>,
}

// ─── HTML Element Node ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ElementNode {
    /// tag name: h1, div, button, input, etc.
    pub tag:      String,
    pub attrs:    AttrList,
    pub children: Vec<Child>,
}

// ─── BlissGeo Nodes ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum GeoChild {
    Shape { kind: String, attrs: AttrList },
    VarDecl { name: String, value: Expr },
    Repeat { binding: String, from: Expr, to: Expr, body: Vec<GeoChild> },
    Comment(String),
}

// ─── Top-Level Declarations ───────────────────────────────────────────────────

/// A complete parsed BlissLang file.
#[derive(Debug, Clone)]
pub enum BlissFile {
    Page(PageNode),
    Section(SectionNode),
    Div(DivNode),
    Article(ArticleNode),
    State(StateNode),
    Model(ModelNode),
    Animation(AnimationNode),
    TypeDef(TypeDefNode),
    ApiRoute(ApiRouteNode),
}

// ─── Output Mode ─────────────────────────────────────────────────────────────

/// Per-page output mode — can override the project-level bliss.config setting.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputMode {
    /// Pre-rendered at build time, served as static HTML.
    Static,
    /// Rendered per-request by the Rust server. Supports {{query.x}} templates.
    Runtime,
    /// Pre-rendered HTML shell + per-request template substitution.
    Hybrid,
}

impl OutputMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "runtime" => OutputMode::Runtime,
            "hybrid"  => OutputMode::Hybrid,
            _         => OutputMode::Static,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputMode::Static  => "static",
            OutputMode::Runtime => "runtime",
            OutputMode::Hybrid  => "hybrid",
        }
    }
}

// ─── Page ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PageNode {
    pub name:      String,
    pub attrs:     AttrList,
    pub route:     Option<String>,
    pub output:    OutputMode,
    pub layout:    Option<String>,
    pub is_layout: bool,
    pub sections:  Vec<PageChild>,
}

#[derive(Debug, Clone)]
pub enum PageChild {
    Include { name: String, attrs: AttrList },
    ForEach { collection: String, binding: String, section: String, attrs: AttrList },
    /// Slot["content"] — marks where a child page's body is injected when this
    /// page is used as a layout
    Slot { name: String },
    Comment(String),
}

// ── Section ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SectionNode {
    pub name:      String,
    pub attrs:     AttrList,
    pub props:     Vec<PropDef>,
    pub lifecycle: LifecycleHooks,
    pub children:  Vec<Child>,
}

// ── Div ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DivNode {
    pub name:      String,
    pub attrs:     AttrList,
    pub props:     Vec<PropDef>,
    pub lifecycle: LifecycleHooks,
    pub children:  Vec<Child>,
}

// ── Article ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ArticleNode {
    pub name:      String,
    pub attrs:     AttrList,
    pub props:     Vec<PropDef>,
    pub children:  Vec<Child>,
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StateNode {
    pub name:    String,
    pub signals: Vec<SignalDef>,
    pub derived: Vec<DerivedDef>,
    pub effects: Vec<EffectDef>,
}

#[derive(Debug, Clone)]
pub struct SignalDef {
    pub name:     String,
    pub ty:       String,
    pub default:  AttrValue,
}

#[derive(Debug, Clone)]
pub struct DerivedDef {
    pub name:    String,
    pub from:    String,
    pub compute: String,
}

#[derive(Debug, Clone)]
pub struct EffectDef {
    pub watch: String,
    pub body:  Vec<WhenClause>,
}

#[derive(Debug, Clone)]
pub struct WhenClause {
    pub cond: AttrValue,
    pub body: Vec<Stmt>,
}

// ── Model ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelNode {
    pub name:   String,
    pub fields: Vec<ModelField>,
}

#[derive(Debug, Clone)]
pub struct ModelField {
    pub name:       String,
    pub ty:         String,
    pub modifiers:  Vec<String>,   // primary, required, auto, unique, etc.
}

// ── Animation ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnimationNode {
    pub name:   String,
    pub frames: Vec<AnimationFrame>,
}

#[derive(Debug, Clone)]
pub struct AnimationFrame {
    /// "0%", "50%", "100%", "from", "to"
    pub at:   String,
    pub props: HashMap<String, String>,
}

// ── Type Definition ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TypeDefNode {
    pub name:   String,
    pub fields: Vec<TypeField>,
}

#[derive(Debug, Clone)]
pub struct TypeField {
    pub name:     String,
    pub ty:       String,
    pub optional: bool,
}

// ── API Route ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ApiRouteNode {
    pub path:   String,
    pub method: HttpMethod,
    pub auth:   AuthRequirement,
    pub roles:  Vec<String>,
    pub body:   Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HttpMethod { Get, Post, Put, Patch, Delete }

#[derive(Debug, Clone, PartialEq)]
pub enum AuthRequirement { None, Optional, Required }

// ─── Helpers ──────────────────────────────────────────────────────────────────

// Extension trait for Vec<Attr>
pub trait AttrListExt {
    fn get_attr(&self, key: &str) -> Option<&AttrValue>;
    fn get_str(&self, key: &str) -> Option<&str>;
    fn get_num(&self, key: &str) -> Option<f64>;
    fn get_bool(&self, key: &str) -> Option<bool>;
}

impl AttrListExt for Vec<Attr> {
    fn get_attr(&self, key: &str) -> Option<&AttrValue> {
        let parts: Vec<&str> = key.split('.').collect();
        self.iter()
            .find(|a| a.key.iter().map(|s| s.as_str()).collect::<Vec<_>>() == parts)
            .map(|a| &a.value)
    }

    fn get_str(&self, key: &str) -> Option<&str> {
        match self.get_attr(key) {
            Some(AttrValue::Str(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn get_num(&self, key: &str) -> Option<f64> {
        match self.get_attr(key) {
            Some(AttrValue::Number(n)) => Some(*n),
            _ => None,
        }
    }

    fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get_attr(key) {
            Some(AttrValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }
}
