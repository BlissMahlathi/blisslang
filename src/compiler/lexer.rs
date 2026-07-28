/// BlissLang Lexer — converts raw source text into a stream of tokens.
///
/// BlissLang is indentation-sensitive. The lexer tracks indent levels and
/// emits synthetic INDENT / DEDENT tokens so the parser never has to count
/// spaces directly.
///
/// Token stream for:
///   BuildSection[name: "Hero"]:
///       h1[text: "Hello"]
///
/// ⟹  Keyword("BuildSection")  LBracket  Ident("name")  Colon
///     String("Hero")  RBracket  Colon  Newline
///     Indent
///     Ident("h1")  LBracket  Ident("text")  Colon  String("Hello")  RBracket  Newline
///     Dedent  Eof

use std::fmt;
use thiserror::Error;

// ─── Token Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ─────────────────────────────────────
    /// "hello world"  — the quotes are stripped
    StringLit(String),
    /// 42  /  3.14  /  -5
    NumberLit(f64),
    /// true | false
    BoolLit(bool),
    /// null
    Null,

    // ── Identifiers & Keywords ────────────────────────
    /// any identifier: Hero, h1, name, style, etc.
    Ident(String),

    // BlissLang keywords (parsed from identifiers for clarity)
    BuildPage,
    BuildSection,
    BuildDiv,
    BuildArticle,
    BuildForm,
    BuildAuth,
    CreateState,
    CreateModel,
    DefineType,
    DefineAnimation,
    DrawCanvas,
    ApiRoute,
    WsHandler,
    IncludeSection,
    UseDiv,
    UsePackage,
    ForEach,
    ShowIf,
    Show,
    ShowElse,
    OnInit,
    OnMount,
    OnUpdate,
    OnDestroy,
    OnWS,
    OnSSE,
    OnBridge,
    OnEvent,
    OnMobile,
    OnTablet,
    OnDesktop,
    ErrorBoundary,
    Slot,
    Into,
    Props,
    Field,
    SubmitButton,
    FormError,
    Describe,
    Test,
    Expect,
    BeforeAll,
    AfterAll,
    BeforeEach,
    AfterEach,
    Render,
    Async,
    Try,
    Catch,
    Finally,
    When,
    Return,
    Repeat,
    Var,
    As,
    From,
    To,
    Track,
    Await,
    And,
    Or,
    Not,

    // ── Punctuation ───────────────────────────────────
    /// [
    LBracket,
    /// ]
    RBracket,
    /// :
    Colon,
    /// ,
    Comma,
    /// .
    Dot,
    /// @
    At,
    /// =
    Equals,
    /// +=
    PlusEq,
    /// -=
    MinusEq,
    /// ==
    EqEq,
    /// !=
    NotEq,
    /// <
    Lt,
    /// >
    Gt,
    /// <=
    LtEq,
    /// >=
    GtEq,
    /// &&
    AndAnd,
    /// ||
    OrOr,
    /// ??
    NullCoal,
    /// !
    Bang,
    /// +
    Plus,
    /// -
    Minus,
    /// *
    Star,
    /// /
    Slash,
    /// %
    Percent,
    /// ?
    Question,
    /// (
    LParen,
    /// )
    RParen,
    /// \{ (start of interpolation inside string)
    InterpStart,
    /// } (end of interpolation)
    InterpEnd,

    // ── Layout / Structure ────────────────────────────
    /// Synthetic token — indentation increased
    Indent,
    /// Synthetic token — indentation decreased
    Dedent,
    /// End of a logical line
    Newline,
    /// End of file
    Eof,

    // ── Comments ─────────────────────────────────────
    /// # this is a comment  (kept for tooling, ignored by parser)
    Comment(String),
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::StringLit(s)  => write!(f, "\"{}\"", s),
            TokenKind::NumberLit(n)  => write!(f, "{}", n),
            TokenKind::BoolLit(b)    => write!(f, "{}", b),
            TokenKind::Null          => write!(f, "null"),
            TokenKind::Ident(s)      => write!(f, "{}", s),
            TokenKind::Indent        => write!(f, "INDENT"),
            TokenKind::Dedent        => write!(f, "DEDENT"),
            TokenKind::Newline       => write!(f, "NEWLINE"),
            TokenKind::Eof           => write!(f, "EOF"),
            TokenKind::LBracket      => write!(f, "["),
            TokenKind::RBracket      => write!(f, "]"),
            TokenKind::Colon         => write!(f, ":"),
            TokenKind::Comma         => write!(f, ","),
            TokenKind::Dot           => write!(f, "."),
            other                    => write!(f, "{:?}", other),
        }
    }
}

// ─── Token with position ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col:  usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Self { kind, line, col }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (line {}, col {})", self.kind, self.line, self.col)
    }
}

// ─── Lex Error ────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum LexError {
    #[error("Unterminated string at line {line}, col {col}")]
    UnterminatedString { line: usize, col: usize },

    #[error("Invalid character '{ch}' at line {line}, col {col}")]
    InvalidChar { ch: char, line: usize, col: usize },

    #[error("Invalid number '{raw}' at line {line}, col {col}")]
    InvalidNumber { raw: String, line: usize, col: usize },

    #[error("Inconsistent indentation at line {line} — use spaces only")]
    MixedIndentation { line: usize },
}

// ─── Lexer ────────────────────────────────────────────────────────────────────

pub struct Lexer {
    source:        Vec<char>,
    pos:           usize,
    line:          usize,
    col:           usize,
    indent_stack:  Vec<usize>,   // stack of indentation levels
    tokens:        Vec<Token>,
    pending_indent: bool,        // true when we just emitted Newline and need to check indent
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source:         source.chars().collect(),
            pos:            0,
            line:           1,
            col:            1,
            indent_stack:   vec![0],
            tokens:         Vec::new(),
            pending_indent: true,
        }
    }

    // ── Public entry point ─────────────────────────────────────────────────

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        while !self.at_end() {
            // Handle indentation at the start of each logical line
            if self.pending_indent {
                self.handle_indent()?;
                self.pending_indent = false;
            }

            self.skip_whitespace_inline();

            if self.at_end() { break; }

            let ch = self.current();

            match ch {
                // Comments
                '#' => self.lex_comment(),

                // Newlines — end logical line
                '\n' => {
                    self.emit(TokenKind::Newline);
                    self.advance();
                    self.pending_indent = true;
                }

                // Skip carriage returns
                '\r' => { self.advance(); }

                // Strings
                '"' => self.lex_string()?,

                // Numbers (including negative)
                c if c.is_ascii_digit() || (c == '-' && self.peek_is_digit()) => {
                    self.lex_number()?;
                }

                // Identifiers and keywords
                c if c.is_alphabetic() || c == '_' => self.lex_ident_or_keyword(),

                // @ prefix for asset paths
                '@' => { self.emit(TokenKind::At); self.advance(); }

                // Punctuation
                '[' => { self.emit(TokenKind::LBracket);  self.advance(); }
                ']' => { self.emit(TokenKind::RBracket);  self.advance(); }
                ':' => { self.emit(TokenKind::Colon);     self.advance(); }
                ',' => { self.emit(TokenKind::Comma);     self.advance(); }
                '.' => { self.emit(TokenKind::Dot);       self.advance(); }
                '(' => { self.emit(TokenKind::LParen);    self.advance(); }
                ')' => { self.emit(TokenKind::RParen);    self.advance(); }
                '?' => {
                    if self.peek() == Some('?') {
                        self.emit(TokenKind::NullCoal); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Question); self.advance();
                    }
                }
                '+' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::PlusEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Plus); self.advance();
                    }
                }
                '-' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::MinusEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Minus); self.advance();
                    }
                }
                '*' => { self.emit(TokenKind::Star);    self.advance(); }
                '/' => { self.emit(TokenKind::Slash);   self.advance(); }
                '%' => { self.emit(TokenKind::Percent); self.advance(); }
                '!' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::NotEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Bang); self.advance();
                    }
                }
                '=' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::EqEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Equals); self.advance();
                    }
                }
                '<' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::LtEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Lt); self.advance();
                    }
                }
                '>' => {
                    if self.peek() == Some('=') {
                        self.emit(TokenKind::GtEq); self.advance(); self.advance();
                    } else {
                        self.emit(TokenKind::Gt); self.advance();
                    }
                }
                '&' => {
                    if self.peek() == Some('&') {
                        self.emit(TokenKind::AndAnd); self.advance(); self.advance();
                    } else {
                        return Err(LexError::InvalidChar { ch: '&', line: self.line, col: self.col });
                    }
                }
                '|' => {
                    if self.peek() == Some('|') {
                        self.emit(TokenKind::OrOr); self.advance(); self.advance();
                    } else {
                        return Err(LexError::InvalidChar { ch: '|', line: self.line, col: self.col });
                    }
                }
                '{' => { self.emit(TokenKind::InterpStart); self.advance(); }
                '}' => { self.emit(TokenKind::InterpEnd);   self.advance(); }

                other => {
                    return Err(LexError::InvalidChar {
                        ch: other, line: self.line, col: self.col
                    });
                }
            }
        }

        // Close any open indentation levels at EOF
        self.close_all_indents();
        self.emit(TokenKind::Eof);
        Ok(self.tokens)
    }

    // ── Indentation handling ───────────────────────────────────────────────

    fn handle_indent(&mut self) -> Result<(), LexError> {
        // Skip blank lines entirely
        while !self.at_end() && self.current() == '\n' {
            self.advance();
        }
        if self.at_end() { return Ok(()); }

        // Count leading spaces on this line
        let start = self.pos;
        let mut spaces = 0usize;
        while !self.at_end() && self.current() == ' ' {
            spaces += 1;
            self.pos += 1;
            self.col += 1;
        }
        // Check for tabs — BlissLang uses spaces only
        if !self.at_end() && self.current() == '\t' {
            return Err(LexError::MixedIndentation { line: self.line });
        }

        let _ = start; // silence unused warning
        let current_level = *self.indent_stack.last().unwrap();

        if spaces > current_level {
            self.indent_stack.push(spaces);
            self.emit(TokenKind::Indent);
        } else {
            while *self.indent_stack.last().unwrap() > spaces {
                self.indent_stack.pop();
                self.emit(TokenKind::Dedent);
            }
        }

        Ok(())
    }

    fn close_all_indents(&mut self) {
        while *self.indent_stack.last().unwrap_or(&0) > 0 {
            self.indent_stack.pop();
            self.emit(TokenKind::Dedent);
        }
    }

    // ── String lexing ──────────────────────────────────────────────────────

    fn lex_string(&mut self) -> Result<(), LexError> {
        let start_line = self.line;
        let start_col  = self.col;
        self.advance(); // consume opening "

        let mut value = String::new();

        loop {
            if self.at_end() || self.current() == '\n' {
                return Err(LexError::UnterminatedString {
                    line: start_line, col: start_col
                });
            }

            let ch = self.current();

            // Escape sequences
            if ch == '\\' {
                self.advance();
                match self.current() {
                    'n'  => { value.push('\n'); self.advance(); }
                    't'  => { value.push('\t'); self.advance(); }
                    'r'  => { value.push('\r'); self.advance(); }
                    '"'  => { value.push('"');  self.advance(); }
                    '\\' => { value.push('\\'); self.advance(); }
                    // \{ starts interpolation marker — emit the string so far
                    '{' => {
                        self.advance();
                        // emit accumulated string part, then InterpStart
                        if !value.is_empty() {
                            let tok = TokenKind::StringLit(value.clone());
                            self.emit(tok);
                            value.clear();
                        }
                        self.emit(TokenKind::InterpStart);
                        // lex the expression inside until }
                        self.lex_interp_expr()?;
                        self.emit(TokenKind::InterpEnd);
                        // continue collecting the rest of the string
                    }
                    other => {
                        value.push('\\');
                        value.push(other);
                        self.advance();
                    }
                }
            } else if ch == '"' {
                // closing quote
                self.advance();
                break;
            } else {
                value.push(ch);
                self.advance();
            }
        }

        self.emit(TokenKind::StringLit(value));
        Ok(())
    }

    /// Lex the expression content inside \{ ... } interpolation.
    /// We recursively call the normal tokenisation for simple identifiers/paths.
    fn lex_interp_expr(&mut self) -> Result<(), LexError> {
        // collect chars until matching }
        let mut depth = 1usize;
        let mut expr  = String::new();

        while !self.at_end() {
            let ch = self.current();
            match ch {
                '{' => { depth += 1; expr.push(ch); self.advance(); }
                '}' => {
                    depth -= 1;
                    if depth == 0 { self.advance(); break; }
                    expr.push(ch); self.advance();
                }
                _ => { expr.push(ch); self.advance(); }
            }
        }

        // Tokenise the inner expression as an identifier path for now.
        // A full expression sub-lexer can be added in v0.2.
        let trimmed = expr.trim().to_string();
        if !trimmed.is_empty() {
            self.emit(TokenKind::Ident(trimmed));
        }
        Ok(())
    }

    // ── Number lexing ──────────────────────────────────────────────────────

    fn lex_number(&mut self) -> Result<(), LexError> {
        let start_line = self.line;
        let start_col  = self.col;
        let mut raw    = String::new();

        if self.current() == '-' {
            raw.push('-');
            self.advance();
        }

        while !self.at_end() && (self.current().is_ascii_digit() || self.current() == '.') {
            raw.push(self.current());
            self.advance();
        }

        match raw.parse::<f64>() {
            Ok(n)  => { self.emit(TokenKind::NumberLit(n)); Ok(()) }
            Err(_) => Err(LexError::InvalidNumber { raw, line: start_line, col: start_col })
        }
    }

    // ── Identifier & keyword lexing ────────────────────────────────────────

    fn lex_ident_or_keyword(&mut self) {
        let mut name = String::new();

        while !self.at_end() && (self.current().is_alphanumeric() || self.current() == '_') {
            name.push(self.current());
            self.advance();
        }

        let kind = Self::keyword_or_ident(name);
        self.emit(kind);
    }

    fn keyword_or_ident(s: String) -> TokenKind {
        match s.as_str() {
            "true"           => TokenKind::BoolLit(true),
            "false"          => TokenKind::BoolLit(false),
            "null"           => TokenKind::Null,
            "BuildPage"      => TokenKind::BuildPage,
            "BuildSection"   => TokenKind::BuildSection,
            "BuildDiv"       => TokenKind::BuildDiv,
            "BuildArticle"   => TokenKind::BuildArticle,
            "BuildForm"      => TokenKind::BuildForm,
            "BuildAuth"      => TokenKind::BuildAuth,
            "CreateState"    => TokenKind::CreateState,
            "CreateModel"    => TokenKind::CreateModel,
            "DefineType"     => TokenKind::DefineType,
            "DefineAnimation"=> TokenKind::DefineAnimation,
            "DrawCanvas"     => TokenKind::DrawCanvas,
            "ApiRoute"       => TokenKind::ApiRoute,
            "WsHandler"      => TokenKind::WsHandler,
            "IncludeSection" => TokenKind::IncludeSection,
            "UseDiv"         => TokenKind::UseDiv,
            "UsePackage"     => TokenKind::UsePackage,
            "ForEach"        => TokenKind::ForEach,
            "ShowIf"         => TokenKind::ShowIf,
            "Show"           => TokenKind::Show,
            "ShowElse"       => TokenKind::ShowElse,
            "OnInit"         => TokenKind::OnInit,
            "OnMount"        => TokenKind::OnMount,
            "OnUpdate"       => TokenKind::OnUpdate,
            "OnDestroy"      => TokenKind::OnDestroy,
            "OnWS"           => TokenKind::OnWS,
            "OnSSE"          => TokenKind::OnSSE,
            "OnBridge"       => TokenKind::OnBridge,
            "OnEvent"        => TokenKind::OnEvent,
            "OnMobile"       => TokenKind::OnMobile,
            "OnTablet"       => TokenKind::OnTablet,
            "OnDesktop"      => TokenKind::OnDesktop,
            "ErrorBoundary"  => TokenKind::ErrorBoundary,
            "Slot"           => TokenKind::Slot,
            "Into"           => TokenKind::Into,
            "Props"          => TokenKind::Props,
            "Field"          => TokenKind::Field,
            "SubmitButton"   => TokenKind::SubmitButton,
            "FormError"      => TokenKind::FormError,
            "Describe"       => TokenKind::Describe,
            "Test"           => TokenKind::Test,
            "Expect"         => TokenKind::Expect,
            "expect"         => TokenKind::Expect,
            "BeforeAll"      => TokenKind::BeforeAll,
            "AfterAll"       => TokenKind::AfterAll,
            "BeforeEach"     => TokenKind::BeforeEach,
            "AfterEach"      => TokenKind::AfterEach,
            "Render"         => TokenKind::Render,
            "Async"          => TokenKind::Async,
            "Try"            => TokenKind::Try,
            "Catch"          => TokenKind::Catch,
            "Finally"        => TokenKind::Finally,
            "when"           => TokenKind::When,
            "return"         => TokenKind::Return,
            "repeat"         => TokenKind::Repeat,
            "var"            => TokenKind::Var,
            "as"             => TokenKind::As,
            "from"           => TokenKind::From,
            "to"             => TokenKind::To,
            "track"          => TokenKind::Track,
            "await"          => TokenKind::Await,
            "and"            => TokenKind::And,
            "or"             => TokenKind::Or,
            "not"            => TokenKind::Not,
            _                => TokenKind::Ident(s),
        }
    }

    // ── Comment lexing ─────────────────────────────────────────────────────

    fn lex_comment(&mut self) {
        let mut text = String::new();
        self.advance(); // consume #
        // optional space
        if !self.at_end() && self.current() == ' ' { self.advance(); }
        while !self.at_end() && self.current() != '\n' {
            text.push(self.current());
            self.advance();
        }
        self.emit(TokenKind::Comment(text));
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    fn current(&self) -> char {
        self.source[self.pos]
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn peek_is_digit(&self) -> bool {
        self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false)
    }

    fn at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col   = 1;
        } else {
            self.col  += 1;
        }
        ch
    }

    fn skip_whitespace_inline(&mut self) {
        while !self.at_end() && (self.current() == ' ' || self.current() == '\t') {
            self.advance();
        }
    }

    fn emit(&mut self, kind: TokenKind) {
        self.tokens.push(Token::new(kind, self.line, self.col));
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Tokenise a BlissLang source string.
/// Returns a Vec<Token> ending with Eof, or a LexError.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Comment(_)))
            .collect()
    }

    #[test]
    fn test_empty() {
        let tokens = tokenize("").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
    }

    #[test]
    fn test_string_literal() {
        let k = kinds("\"Hello World\"");
        assert!(matches!(&k[0], TokenKind::StringLit(s) if s == "Hello World"));
    }

    #[test]
    fn test_number_literal() {
        let k = kinds("42");
        assert!(matches!(&k[0], TokenKind::NumberLit(n) if *n == 42.0));
    }

    #[test]
    fn test_bool_literals() {
        let k = kinds("true false");
        assert_eq!(k[0], TokenKind::BoolLit(true));
        assert_eq!(k[1], TokenKind::BoolLit(false));
    }

    #[test]
    fn test_keywords() {
        let k = kinds("BuildSection ForEach ShowIf");
        assert_eq!(k[0], TokenKind::BuildSection);
        assert_eq!(k[1], TokenKind::ForEach);
        assert_eq!(k[2], TokenKind::ShowIf);
    }

    #[test]
    fn test_bracket_colon() {
        let k = kinds("[name: \"Hero\"]");
        assert_eq!(k[0], TokenKind::LBracket);
        assert!(matches!(&k[1], TokenKind::Ident(s) if s == "name"));
        assert_eq!(k[2], TokenKind::Colon);
        assert!(matches!(&k[3], TokenKind::StringLit(s) if s == "Hero"));
        assert_eq!(k[4], TokenKind::RBracket);
    }

    #[test]
    fn test_indent_dedent() {
        let src = "BuildSection[name: \"Hero\"]:\n    h1[text: \"Hello\"]\n";
        let k = kinds(src);
        // Should contain Indent before h1 and Dedent after
        assert!(k.contains(&TokenKind::Indent));
        assert!(k.contains(&TokenKind::Dedent));
    }

    #[test]
    fn test_operators() {
        let k = kinds("== != <= >= && || ??");
        assert_eq!(k[0], TokenKind::EqEq);
        assert_eq!(k[1], TokenKind::NotEq);
        assert_eq!(k[2], TokenKind::LtEq);
        assert_eq!(k[3], TokenKind::GtEq);
        assert_eq!(k[4], TokenKind::AndAnd);
        assert_eq!(k[5], TokenKind::OrOr);
        assert_eq!(k[6], TokenKind::NullCoal);
    }

    #[test]
    fn test_comment_skipped_in_kinds() {
        let k = kinds("# this is a comment\nBuiltSection");
        // Comment filtered out, only Ident remains (BuildSection not matched = Ident)
        assert!(!k.iter().any(|t| matches!(t, TokenKind::Comment(_))));
    }

    #[test]
    fn test_dot_attr_key() {
        // style.tailwind is lexed as: Ident("style") Dot Ident("tailwind")
        let k = kinds("style.tailwind");
        assert!(matches!(&k[0], TokenKind::Ident(s) if s == "style"));
        assert_eq!(k[1], TokenKind::Dot);
        assert!(matches!(&k[2], TokenKind::Ident(s) if s == "tailwind"));
    }

    #[test]
    fn test_null_coal_operator() {
        let k = kinds("a ?? b");
        assert_eq!(k[1], TokenKind::NullCoal);
    }
}
