use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct CalculatorPlugin;

#[async_trait]
impl Plugin for CalculatorPlugin {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Evaluate mathematical expressions"
    }

    fn keyword(&self) -> Option<&str> {
        Some("=")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let expr = q.raw.strip_prefix('=').unwrap_or(&q.raw).trim();
        match evaluate(expr) {
            Some(result) => vec![QueryResult {
                id: format!("calc:{}", expr),
                title: format!("{} = {}", expr, result),
                subtitle: Some("Press Enter to copy result".to_string()),
                icon: Some("🧮".to_string()),
                score: 100,
                action_type: "copy".to_string(),
                action_data: result.to_string(),
                source: None,
            }],
            None => vec![],
        }
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "calculator",
                "description": "Evaluate a math expression",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "expression": { "type": "string", "description": "Math expression to evaluate" }
                    },
                    "required": ["expression"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let expr = args["expression"].as_str().unwrap_or("");
        match evaluate(expr) {
            Some(r) => r.to_string(),
            None => "Could not evaluate expression".to_string(),
        }
    }
}

/// Simple recursive descent math parser supporting +, -, *, /, ^, parentheses
pub fn evaluate(expr: &str) -> Option<f64> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let result = parse_expr(&tokens, &mut pos)?;
    if pos == tokens.len() {
        Some(result)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Option<Vec<Token>> {
    let mut tokens = vec![];
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '0'..='9' | '.' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Num(num.parse().ok()?));
            }
            '+' => {
                tokens.push(Token::Plus);
                chars.next();
            }
            '-' => {
                tokens.push(Token::Minus);
                chars.next();
            }
            '*' => {
                tokens.push(Token::Star);
                chars.next();
            }
            '/' => {
                tokens.push(Token::Slash);
                chars.next();
            }
            '^' => {
                tokens.push(Token::Caret);
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn parse_expr(tokens: &[Token], pos: &mut usize) -> Option<f64> {
    let mut left = parse_term(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                left += parse_term(tokens, pos)?;
            }
            Token::Minus => {
                *pos += 1;
                left -= parse_term(tokens, pos)?;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_term(tokens: &[Token], pos: &mut usize) -> Option<f64> {
    let mut left = parse_power(tokens, pos)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Star => {
                *pos += 1;
                left *= parse_power(tokens, pos)?;
            }
            Token::Slash => {
                *pos += 1;
                let r = parse_power(tokens, pos)?;
                if r == 0.0 {
                    return None;
                }
                left /= r;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_power(tokens: &[Token], pos: &mut usize) -> Option<f64> {
    let base = parse_unary(tokens, pos)?;
    if *pos < tokens.len() && tokens[*pos] == Token::Caret {
        *pos += 1;
        let exp = parse_unary(tokens, pos)?;
        Some(base.powf(exp))
    } else {
        Some(base)
    }
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Option<f64> {
    if *pos < tokens.len() && tokens[*pos] == Token::Minus {
        *pos += 1;
        return Some(-parse_primary(tokens, pos)?);
    }
    parse_primary(tokens, pos)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Option<f64> {
    if *pos >= tokens.len() {
        return None;
    }
    match &tokens[*pos] {
        Token::Num(n) => {
            let v = *n;
            *pos += 1;
            Some(v)
        }
        Token::LParen => {
            *pos += 1;
            let v = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos] == Token::RParen {
                *pos += 1;
                Some(v)
            } else {
                None
            }
        }
        _ => None,
    }
}
