/// BlissGeo Math Evaluator
///
/// Evaluates mathematical expressions used in BlissGeo geometry.
/// Supports: sin, cos, tan, sqrt, abs, pow, log, PI, E, and arithmetic.
/// No external crates — pure Rust.
///
/// Used by: plot[], parametric[], polar[], spiral[], regularPolygon[], repeat[]

use std::collections::HashMap;

// ─── Eval Error ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct EvalError(pub String);

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Math eval error: {}", self.0)
    }
}

type EvalResult = Result<f64, EvalError>;

// ─── Tokeniser ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum MathToken {
    Number(f64),
    Ident(String),
    Plus, Minus, Star, Slash, Caret, Percent,
    LParen, RParen, Comma,
}

fn tokenise(expr: &str) -> Result<Vec<MathToken>, EvalError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => { i += 1; }
            '+' => { tokens.push(MathToken::Plus);   i += 1; }
            '-' => { tokens.push(MathToken::Minus);  i += 1; }
            '*' => { tokens.push(MathToken::Star);   i += 1; }
            '/' => { tokens.push(MathToken::Slash);  i += 1; }
            '^' => { tokens.push(MathToken::Caret);  i += 1; }
            '%' => { tokens.push(MathToken::Percent);i += 1; }
            '(' => { tokens.push(MathToken::LParen); i += 1; }
            ')' => { tokens.push(MathToken::RParen); i += 1; }
            ',' => { tokens.push(MathToken::Comma);  i += 1; }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                let n = s.parse::<f64>().map_err(|_| EvalError(format!("Invalid number: {}", s)))?;
                tokens.push(MathToken::Number(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                tokens.push(MathToken::Ident(name));
            }
            c => return Err(EvalError(format!("Unknown character: '{}'", c))),
        }
    }
    Ok(tokens)
}

// ─── Parser / Evaluator ───────────────────────────────────────────────────────

struct Evaluator<'a> {
    tokens: Vec<MathToken>,
    pos:    usize,
    vars:   &'a HashMap<String, f64>,
}

impl<'a> Evaluator<'a> {
    fn new(tokens: Vec<MathToken>, vars: &'a HashMap<String, f64>) -> Self {
        Self { tokens, pos: 0, vars }
    }

    fn peek(&self) -> Option<&MathToken> {
        self.tokens.get(self.pos)
    }

    fn consume(&mut self) -> Option<MathToken> {
        let tok = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        tok
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    // expr → term (('+' | '-') term)*
    fn expr(&mut self) -> EvalResult {
        let mut val = self.term()?;
        loop {
            match self.peek() {
                Some(MathToken::Plus)  => { self.consume(); val += self.term()?; }
                Some(MathToken::Minus) => { self.consume(); val -= self.term()?; }
                _ => break,
            }
        }
        Ok(val)
    }

    // term → power (('*' | '/' | '%') power)*
    fn term(&mut self) -> EvalResult {
        let mut val = self.power()?;
        loop {
            match self.peek() {
                Some(MathToken::Star)    => { self.consume(); val *= self.power()?; }
                Some(MathToken::Slash)   => {
                    self.consume();
                    let d = self.power()?;
                    if d == 0.0 { return Err(EvalError("Division by zero".into())); }
                    val /= d;
                }
                Some(MathToken::Percent) => {
                    self.consume();
                    let d = self.power()?;
                    if d == 0.0 { return Err(EvalError("Modulo by zero".into())); }
                    val %= d;
                }
                _ => break,
            }
        }
        Ok(val)
    }

    // power → unary ('^' power)?   (right-associative)
    fn power(&mut self) -> EvalResult {
        let base = self.unary()?;
        if let Some(MathToken::Caret) = self.peek() {
            self.consume();
            let exp = self.power()?;
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    // unary → '-' primary | primary
    fn unary(&mut self) -> EvalResult {
        if let Some(MathToken::Minus) = self.peek() {
            self.consume();
            Ok(-self.primary()?)
        } else {
            self.primary()
        }
    }

    // primary → number | constant | function_call | variable | '(' expr ')'
    fn primary(&mut self) -> EvalResult {
        match self.consume() {
            Some(MathToken::Number(n)) => Ok(n),

            Some(MathToken::LParen) => {
                let val = self.expr()?;
                match self.consume() {
                    Some(MathToken::RParen) => Ok(val),
                    _ => Err(EvalError("Expected closing ')'".into())),
                }
            }

            Some(MathToken::Ident(name)) => {
                // Check for function call
                if let Some(MathToken::LParen) = self.peek() {
                    self.consume(); // consume '('
                    let args = self.arg_list()?;
                    match self.consume() {
                        Some(MathToken::RParen) => {}
                        _ => return Err(EvalError(format!("Expected ')' after {}()", name))),
                    }
                    self.call_fn(&name, args)
                } else {
                    // Variable or constant
                    self.resolve(&name)
                }
            }

            Some(tok) => Err(EvalError(format!("Unexpected token: {:?}", tok))),
            None      => Err(EvalError("Unexpected end of expression".into())),
        }
    }

    fn arg_list(&mut self) -> Result<Vec<f64>, EvalError> {
        let mut args = Vec::new();
        if let Some(MathToken::RParen) = self.peek() {
            return Ok(args); // empty arg list
        }
        args.push(self.expr()?);
        while let Some(MathToken::Comma) = self.peek() {
            self.consume();
            args.push(self.expr()?);
        }
        Ok(args)
    }

    fn resolve(&self, name: &str) -> EvalResult {
        match name {
            "PI" | "pi" => Ok(std::f64::consts::PI),
            "E"  | "e"  => Ok(std::f64::consts::E),
            "TAU"| "tau"=> Ok(std::f64::consts::TAU),
            "INF"       => Ok(f64::INFINITY),
            _ => self.vars.get(name)
                    .copied()
                    .ok_or_else(|| EvalError(format!("Unknown variable: '{}'", name)))
        }
    }

    fn call_fn(&self, name: &str, args: Vec<f64>) -> EvalResult {
        let a = |n: usize| args.get(n).copied()
            .ok_or_else(|| EvalError(format!("{}() missing arg {}", name, n)));

        match name {
            // Trig
            "sin"   => Ok(a(0)?.sin()),
            "cos"   => Ok(a(0)?.cos()),
            "tan"   => Ok(a(0)?.tan()),
            "asin"  => Ok(a(0)?.asin()),
            "acos"  => Ok(a(0)?.acos()),
            "atan"  => Ok(a(0)?.atan()),
            "atan2" => Ok(a(0)?.atan2(a(1)?)),
            "sinh"  => Ok(a(0)?.sinh()),
            "cosh"  => Ok(a(0)?.cosh()),
            "tanh"  => Ok(a(0)?.tanh()),

            // Exponential / logarithm
            "sqrt"  => Ok(a(0)?.sqrt()),
            "cbrt"  => Ok(a(0)?.cbrt()),
            "exp"   => Ok(a(0)?.exp()),
            "ln"    => Ok(a(0)?.ln()),
            "log"   => {
                if args.len() == 2 { Ok(a(0)?.log(a(1)?)) }
                else               { Ok(a(0)?.log10()) }
            }
            "log2"  => Ok(a(0)?.log2()),
            "log10" => Ok(a(0)?.log10()),
            "pow"   => Ok(a(0)?.powf(a(1)?)),

            // Rounding
            "floor" => Ok(a(0)?.floor()),
            "ceil"  => Ok(a(0)?.ceil()),
            "round" => Ok(a(0)?.round()),
            "abs"   => Ok(a(0)?.abs()),
            "sign"  => Ok(a(0)?.signum()),

            // Min / Max / Clamp
            "min"   => Ok(a(0)?.min(a(1)?)),
            "max"   => Ok(a(0)?.max(a(1)?)),
            "clamp" => Ok(a(0)?.clamp(a(1)?, a(2)?)),

            // Interpolation
            "lerp"  => { let (a0, a1, t) = (a(0)?, a(1)?, a(2)?); Ok(a0 + (a1 - a0) * t) }
            "mix"   => { let (a0, a1, t) = (a(0)?, a(1)?, a(2)?); Ok(a0 * (1.0 - t) + a1 * t) }

            // Utility
            "fract" => Ok(a(0)?.fract()),
            "deg"   => Ok(a(0)?.to_radians()),
            "rad"   => Ok(a(0)?.to_degrees()),
            "step"  => Ok(if a(1)? >= a(0)? { 1.0 } else { 0.0 }),
            "mod"   => { let d = a(1)?; if d == 0.0 { return Err(EvalError("mod by zero".into())); } Ok(a(0)? % d) }

            // vertex(cx, cy, r, i, n) — nth vertex of a regular polygon
            "vertex" => {
                let (cx, cy, r, i, n) = (a(0)?, a(1)?, a(2)?, a(3)?, a(4)?);
                let angle = 2.0 * std::f64::consts::PI * i / n - std::f64::consts::PI / 2.0;
                Ok(cx + r * angle.cos() + cy + r * angle.sin())
            }

            other => Err(EvalError(format!("Unknown function: '{}()'", other))),
        }
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Evaluate a math expression with a set of named variables.
///
/// Example:
///   eval("sin(x) * 100", &[("x", 1.5708)]) → ~100.0
pub fn eval(expr: &str, vars: &[(&str, f64)]) -> EvalResult {
    let var_map: HashMap<String, f64> = vars.iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();

    let tokens = tokenise(expr)?;
    let mut evaluator = Evaluator::new(tokens, &var_map);
    let result = evaluator.expr()?;

    if !evaluator.at_end() {
        return Err(EvalError(format!(
            "Unexpected token at position {}: {:?}",
            evaluator.pos,
            evaluator.tokens.get(evaluator.pos)
        )));
    }

    Ok(result)
}

/// Evaluate with a pre-built HashMap (more efficient for repeated calls)
pub fn eval_with_map(expr: &str, vars: &HashMap<String, f64>) -> EvalResult {
    let tokens = tokenise(expr)?;
    let mut evaluator = Evaluator::new(tokens, vars);
    evaluator.expr()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str) -> f64 { eval(s, &[]).unwrap() }
    fn ev(s: &str, vars: &[(&str, f64)]) -> f64 { eval(s, vars).unwrap() }

    #[test] fn test_arithmetic()      { assert_eq!(e("2 + 3"), 5.0); assert_eq!(e("10 - 3"), 7.0); assert_eq!(e("4 * 5"), 20.0); assert_eq!(e("9 / 3"), 3.0); }
    #[test] fn test_precedence()      { assert_eq!(e("2 + 3 * 4"), 14.0); assert_eq!(e("(2 + 3) * 4"), 20.0); }
    #[test] fn test_power()           { assert_eq!(e("2 ^ 10"), 1024.0); }
    #[test] fn test_unary_minus()     { assert_eq!(e("-5"), -5.0); assert_eq!(e("-(3 + 2)"), -5.0); }
    #[test] fn test_constants()       { let pi = e("PI"); assert!((pi - std::f64::consts::PI).abs() < 1e-10); }
    #[test] fn test_trig()            { let s = e("sin(PI / 2)"); assert!((s - 1.0).abs() < 1e-10); let c = e("cos(0)"); assert!((c - 1.0).abs() < 1e-10); }
    #[test] fn test_sqrt()            { assert!((e("sqrt(16)") - 4.0).abs() < 1e-10); }
    #[test] fn test_variables()       { let v = ev("x * 2 + y", &[("x", 3.0), ("y", 5.0)]); assert_eq!(v, 11.0); }
    #[test] fn test_nested_fns()      { let v = e("sqrt(sin(PI/2) ^ 2 + cos(0) ^ 2)"); assert!((v - std::f64::consts::SQRT_2).abs() < 1e-8); }
    #[test] fn test_lerp()            { assert_eq!(ev("lerp(0, 10, 0.5)", &[]), 5.0); }
    #[test] fn test_clamp()           { assert_eq!(ev("clamp(15, 0, 10)", &[]), 10.0); }
    #[test] fn test_division_by_zero(){ assert!(eval("1 / 0", &[]).is_err()); }
    #[test] fn test_unknown_var()     { assert!(eval("foo + 1", &[]).is_err()); }
    #[test] fn test_trig_parametric() {
        // Simulate a parametric plot step: x = cos(t)*r, y = sin(t)*r
        let t = 0.0_f64;
        let r = 100.0_f64;
        let x = eval("cos(t) * r", &[("t", t), ("r", r)]).unwrap();
        let y = eval("sin(t) * r", &[("t", t), ("r", r)]).unwrap();
        assert!((x - 100.0).abs() < 1e-10);
        assert!((y - 0.0).abs() < 1e-10);
    }
}
