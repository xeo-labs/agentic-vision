//! WQL parser — recursive descent parser for the Web Query Language.
//!
//! Grammar:
//! ```text
//! query := SELECT fields FROM model [JOIN ...] [WHERE ...] [ACROSS ...] [ORDER BY ...] [LIMIT n]
//! fields := field (',' field)*
//! field := name [AS alias] | temporal_func
//! temporal_func := name '_' duration '_ago' | name '_trend' | 'predicted_' name '_' duration
//! model := IDENTIFIER
//! where := WHERE expr
//! expr := comparison ((AND | OR) comparison)*
//! comparison := field op value
//! op := '=' | '<' | '>' | '<=' | '>=' | '!='
//! across := ACROSS domain (',' domain)*
//! order := ORDER BY field (ASC | DESC)
//! limit := LIMIT number
//! ```

use anyhow::{bail, Result};
use chrono::Duration;
use serde::{Deserialize, Serialize};

/// A parsed WQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WqlQuery {
    /// Fields to select.
    pub select: Vec<SelectField>,
    /// Model type to query.
    pub from: ModelRef,
    /// Join clauses.
    pub joins: Vec<JoinClause>,
    /// WHERE conditions.
    pub where_clause: Option<WhereExpr>,
    /// ACROSS domain filter.
    pub across: Option<Vec<String>>,
    /// ORDER BY clause.
    pub order_by: Option<Vec<OrderByField>>,
    /// LIMIT clause.
    pub limit: Option<usize>,
}

/// A selected field with optional alias and temporal function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectField {
    /// Field name.
    pub name: String,
    /// Optional alias.
    pub alias: Option<String>,
    /// Temporal function, if any.
    pub temporal_func: Option<TemporalFunc>,
}

/// Temporal function applied to a field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemporalFunc {
    /// Value N days ago (e.g., price_30d_ago).
    ValueAgo(i64),
    /// Trend direction (e.g., price_trend).
    Trend,
    /// Predicted value in N days (e.g., predicted_price_7d).
    Predicted(i64),
    /// Best historic value.
    BestHistoric,
    /// Domain with the best historic value.
    BestHistoricDomain,
}

/// Model reference in FROM clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub name: String,
}

/// JOIN clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinClause {
    pub target_model: String,
    pub on_field: String,
    pub equals_field: String,
}

/// WHERE expression (AND/OR tree of comparisons).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WhereExpr {
    /// Simple comparison: field op value.
    Comparison {
        field: String,
        op: ComparisonOp,
        value: WqlValue,
    },
    /// AND of two expressions.
    And(Box<WhereExpr>, Box<WhereExpr>),
    /// OR of two expressions.
    Or(Box<WhereExpr>, Box<WhereExpr>),
}

/// Comparison operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComparisonOp {
    Eq,
    Lt,
    Gt,
    Lte,
    Gte,
    Neq,
}

/// A literal value in a WQL expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WqlValue {
    Float(f64),
    Integer(i64),
    String(String),
    Bool(bool),
}

/// ORDER BY field with direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderByField {
    pub field: String,
    pub ascending: bool,
}

/// Token for the parser.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Keyword(String), // SELECT, FROM, WHERE, etc.
    Ident(String),   // field/model names
    Number(f64),
    StringLit(String),
    Op(String), // =, <, >, <=, >=, !=
    Comma,
    Star,
    Eof,
}

/// Parse a WQL query string into an AST.
pub fn parse(query: &str) -> Result<WqlQuery> {
    let tokens = tokenize(query)?;
    let mut pos = 0;

    // SELECT
    expect_keyword(&tokens, &mut pos, "SELECT")?;
    let select = parse_select_fields(&tokens, &mut pos)?;

    // FROM
    expect_keyword(&tokens, &mut pos, "FROM")?;
    let from = parse_model_ref(&tokens, &mut pos)?;

    // Optional JOIN
    let mut joins = Vec::new();
    while peek_keyword(&tokens, pos, "JOIN") {
        pos += 1; // consume JOIN
        let join = parse_join(&tokens, &mut pos)?;
        joins.push(join);
    }

    // Optional WHERE
    let where_clause = if peek_keyword(&tokens, pos, "WHERE") {
        pos += 1;
        Some(parse_where_expr(&tokens, &mut pos)?)
    } else {
        None
    };

    // Optional ACROSS
    let across = if peek_keyword(&tokens, pos, "ACROSS") {
        pos += 1;
        Some(parse_domain_list(&tokens, &mut pos)?)
    } else {
        None
    };

    // Optional ORDER BY
    let order_by = if peek_keyword(&tokens, pos, "ORDER") {
        pos += 1;
        expect_keyword(&tokens, &mut pos, "BY")?;
        Some(parse_order_by(&tokens, &mut pos)?)
    } else {
        None
    };

    // Optional LIMIT
    let limit = if peek_keyword(&tokens, pos, "LIMIT") {
        pos += 1;
        Some(parse_limit(&tokens, &mut pos)?)
    } else {
        None
    };

    Ok(WqlQuery {
        select,
        from,
        joins,
        where_clause,
        across,
        order_by,
        limit,
    })
}

// ── Tokenizer ──

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip whitespace
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // String literal
        if chars[i] == '\'' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            if i >= chars.len() {
                bail!("unterminated string literal at position {start}");
            }
            let s: String = chars[start..i].iter().collect();
            tokens.push(Token::StringLit(s));
            i += 1; // consume closing quote
            continue;
        }

        // Number
        if chars[i].is_ascii_digit()
            || (chars[i] == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            if chars[i] == '-' {
                i += 1;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let num_str: String = chars[start..i].iter().collect();
            let num: f64 = num_str
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid number: {num_str}"))?;
            tokens.push(Token::Number(num));
            continue;
        }

        // Operators
        if chars[i] == '<' || chars[i] == '>' || chars[i] == '!' || chars[i] == '=' {
            let start = i;
            i += 1;
            if i < chars.len() && chars[i] == '=' {
                i += 1;
            }
            let op: String = chars[start..i].iter().collect();
            tokens.push(Token::Op(op));
            continue;
        }

        // Comma
        if chars[i] == ',' {
            tokens.push(Token::Comma);
            i += 1;
            continue;
        }

        // Star
        if chars[i] == '*' {
            tokens.push(Token::Star);
            i += 1;
            continue;
        }

        // Identifier or keyword
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_uppercase();

            // Check if it's a keyword
            match upper.as_str() {
                "SELECT" | "FROM" | "WHERE" | "JOIN" | "ON" | "AND" | "OR" | "ORDER" | "BY"
                | "ASC" | "DESC" | "LIMIT" | "ACROSS" | "AS" | "TRUE" | "FALSE" => {
                    if upper == "TRUE" {
                        tokens.push(Token::Number(1.0));
                    } else if upper == "FALSE" {
                        tokens.push(Token::Number(0.0));
                    } else {
                        tokens.push(Token::Keyword(upper));
                    }
                }
                _ => tokens.push(Token::Ident(word)),
            }
            continue;
        }

        bail!("unexpected character '{}' at position {i}", chars[i]);
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

// ── Parser helpers ──

fn expect_keyword(tokens: &[Token], pos: &mut usize, keyword: &str) -> Result<()> {
    if *pos >= tokens.len() {
        bail!("expected '{keyword}' but reached end of query");
    }
    match &tokens[*pos] {
        Token::Keyword(k) if k == keyword => {
            *pos += 1;
            Ok(())
        }
        other => bail!("expected '{keyword}' at position {pos}, found {other:?}"),
    }
}

fn peek_keyword(tokens: &[Token], pos: usize, keyword: &str) -> bool {
    matches!(&tokens.get(pos), Some(Token::Keyword(k)) if k == keyword)
}

fn parse_select_fields(tokens: &[Token], pos: &mut usize) -> Result<Vec<SelectField>> {
    let mut fields = Vec::new();

    // Handle SELECT *
    if matches!(tokens.get(*pos), Some(Token::Star)) {
        *pos += 1;
        fields.push(SelectField {
            name: "*".to_string(),
            alias: None,
            temporal_func: None,
        });
        return Ok(fields);
    }

    loop {
        let field = parse_single_field(tokens, pos)?;
        fields.push(field);

        if !matches!(tokens.get(*pos), Some(Token::Comma)) {
            break;
        }
        *pos += 1; // consume comma
    }

    Ok(fields)
}

fn parse_single_field(tokens: &[Token], pos: &mut usize) -> Result<SelectField> {
    let name = match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            name.clone()
        }
        other => bail!("expected field name, found {other:?}"),
    };

    // Check for temporal function patterns
    let temporal_func = parse_temporal_from_name(&name);

    // Check for AS alias
    let alias = if peek_keyword(tokens, *pos, "AS") {
        *pos += 1;
        match tokens.get(*pos) {
            Some(Token::Ident(alias)) => {
                *pos += 1;
                Some(alias.clone())
            }
            _ => None,
        }
    } else {
        None
    };

    let base_name = if temporal_func.is_some() {
        extract_base_field_name(&name)
    } else {
        name
    };

    Ok(SelectField {
        name: base_name,
        alias,
        temporal_func,
    })
}

/// Parse temporal function from field name patterns.
fn parse_temporal_from_name(name: &str) -> Option<TemporalFunc> {
    // price_30d_ago → ValueAgo(30)
    if let Some(without_ago) = name.strip_suffix("_ago") {
        if let Some(idx) = without_ago.rfind('_') {
            let duration_part = &without_ago[idx + 1..];
            if let Some(stripped) = duration_part.strip_suffix('d') {
                if let Ok(days) = stripped.parse::<i64>() {
                    return Some(TemporalFunc::ValueAgo(days));
                }
            }
        }
    }

    // price_trend → Trend
    if name.ends_with("_trend") {
        return Some(TemporalFunc::Trend);
    }

    // predicted_price_7d → Predicted(7)
    if let Some(rest) = name.strip_prefix("predicted_") {
        if let Some(idx) = rest.rfind('_') {
            let duration_part = &rest[idx + 1..];
            if let Some(stripped) = duration_part.strip_suffix('d') {
                if let Ok(days) = stripped.parse::<i64>() {
                    return Some(TemporalFunc::Predicted(days));
                }
            }
        }
    }

    // best_historic_price → BestHistoric
    if name.starts_with("best_historic_") {
        if name.ends_with("_domain") {
            return Some(TemporalFunc::BestHistoricDomain);
        }
        return Some(TemporalFunc::BestHistoric);
    }

    None
}

/// Extract the base field name from a temporal function name.
fn extract_base_field_name(name: &str) -> String {
    if let Some(without_ago) = name.strip_suffix("_ago") {
        if let Some(idx) = without_ago.rfind('_') {
            return without_ago[..idx].to_string();
        }
    }
    if let Some(stripped) = name.strip_suffix("_trend") {
        return stripped.to_string();
    }
    if let Some(rest) = name.strip_prefix("predicted_") {
        if let Some(idx) = rest.rfind('_') {
            return rest[..idx].to_string();
        }
    }
    if let Some(stripped) = name.strip_prefix("best_historic_") {
        return stripped.to_string();
    }
    name.to_string()
}

fn parse_model_ref(tokens: &[Token], pos: &mut usize) -> Result<ModelRef> {
    match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            Ok(ModelRef { name: name.clone() })
        }
        other => bail!("expected model name, found {other:?}"),
    }
}

fn parse_join(tokens: &[Token], pos: &mut usize) -> Result<JoinClause> {
    let target = match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            name.clone()
        }
        other => bail!("expected model name after JOIN, found {other:?}"),
    };

    expect_keyword(tokens, pos, "ON")?;

    let on_field = match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            name.clone()
        }
        other => bail!("expected field name after ON, found {other:?}"),
    };

    // Expect '='
    match tokens.get(*pos) {
        Some(Token::Op(op)) if op == "=" => {
            *pos += 1;
        }
        other => bail!("expected '=' in JOIN ON, found {other:?}"),
    }

    let equals_field = match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            name.clone()
        }
        other => bail!("expected field name after '=', found {other:?}"),
    };

    Ok(JoinClause {
        target_model: target,
        on_field,
        equals_field,
    })
}

fn parse_where_expr(tokens: &[Token], pos: &mut usize) -> Result<WhereExpr> {
    let left = parse_comparison(tokens, pos)?;

    // Check for AND/OR
    if peek_keyword(tokens, *pos, "AND") {
        *pos += 1;
        let right = parse_where_expr(tokens, pos)?;
        return Ok(WhereExpr::And(Box::new(left), Box::new(right)));
    }
    if peek_keyword(tokens, *pos, "OR") {
        *pos += 1;
        let right = parse_where_expr(tokens, pos)?;
        return Ok(WhereExpr::Or(Box::new(left), Box::new(right)));
    }

    Ok(left)
}

fn parse_comparison(tokens: &[Token], pos: &mut usize) -> Result<WhereExpr> {
    let field = match tokens.get(*pos) {
        Some(Token::Ident(name)) => {
            *pos += 1;
            name.clone()
        }
        other => bail!("expected field name in WHERE, found {other:?}"),
    };

    let op = match tokens.get(*pos) {
        Some(Token::Op(op)) => {
            let parsed = match op.as_str() {
                "=" => ComparisonOp::Eq,
                "<" => ComparisonOp::Lt,
                ">" => ComparisonOp::Gt,
                "<=" => ComparisonOp::Lte,
                ">=" => ComparisonOp::Gte,
                "!=" => ComparisonOp::Neq,
                _ => bail!("unknown operator: {op}"),
            };
            *pos += 1;
            parsed
        }
        other => bail!("expected operator, found {other:?}"),
    };

    let value = match tokens.get(*pos) {
        Some(Token::Number(n)) => {
            *pos += 1;
            if *n == (*n as i64) as f64 {
                WqlValue::Integer(*n as i64)
            } else {
                WqlValue::Float(*n)
            }
        }
        Some(Token::StringLit(s)) => {
            *pos += 1;
            WqlValue::String(s.clone())
        }
        other => bail!("expected value in comparison, found {other:?}"),
    };

    Ok(WhereExpr::Comparison { field, op, value })
}

fn parse_domain_list(tokens: &[Token], pos: &mut usize) -> Result<Vec<String>> {
    let mut domains = Vec::new();
    loop {
        match tokens.get(*pos) {
            Some(Token::Ident(name)) => {
                *pos += 1;
                domains.push(name.clone());
            }
            other => bail!("expected domain name, found {other:?}"),
        }
        if !matches!(tokens.get(*pos), Some(Token::Comma)) {
            break;
        }
        *pos += 1;
    }
    Ok(domains)
}

fn parse_order_by(tokens: &[Token], pos: &mut usize) -> Result<Vec<OrderByField>> {
    let mut fields = Vec::new();
    loop {
        let field = match tokens.get(*pos) {
            Some(Token::Ident(name)) => {
                *pos += 1;
                name.clone()
            }
            other => bail!("expected field name in ORDER BY, found {other:?}"),
        };

        let ascending = if peek_keyword(tokens, *pos, "DESC") {
            *pos += 1;
            false
        } else if peek_keyword(tokens, *pos, "ASC") {
            *pos += 1;
            true
        } else {
            true // default ascending
        };

        fields.push(OrderByField { field, ascending });

        if !matches!(tokens.get(*pos), Some(Token::Comma)) {
            break;
        }
        *pos += 1;
    }
    Ok(fields)
}

fn parse_limit(tokens: &[Token], pos: &mut usize) -> Result<usize> {
    match tokens.get(*pos) {
        Some(Token::Number(n)) => {
            *pos += 1;
            Ok(*n as usize)
        }
        other => bail!("expected number after LIMIT, found {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_select() {
        let q = parse("SELECT name, price FROM Product LIMIT 10").unwrap();
        assert_eq!(q.select.len(), 2);
        assert_eq!(q.from.name, "Product");
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn test_parse_where_clause() {
        let q = parse("SELECT name FROM Product WHERE price < 200").unwrap();
        assert!(q.where_clause.is_some());

        if let Some(WhereExpr::Comparison {
            field,
            op,
            value: _,
        }) = &q.where_clause
        {
            assert_eq!(field, "price");
            assert_eq!(*op, ComparisonOp::Lt);
        } else {
            panic!("expected comparison");
        }
    }

    #[test]
    fn test_parse_where_and() {
        let q = parse("SELECT name FROM Product WHERE price < 200 AND rating > 4.0").unwrap();
        assert!(matches!(q.where_clause, Some(WhereExpr::And(_, _))));
    }

    #[test]
    fn test_parse_order_by() {
        let q = parse("SELECT name, price FROM Product ORDER BY price ASC LIMIT 10").unwrap();
        assert!(q.order_by.is_some());
        let order = q.order_by.unwrap();
        assert_eq!(order[0].field, "price");
        assert!(order[0].ascending);
    }

    #[test]
    fn test_parse_across() {
        let q = parse("SELECT name FROM Product ACROSS amazon.com, bestbuy.com LIMIT 10").unwrap();
        assert!(q.across.is_some());
        let domains = q.across.unwrap();
        assert_eq!(domains.len(), 2);
    }

    #[test]
    fn test_parse_join() {
        let q =
            parse("SELECT name FROM Product JOIN Review ON Review.product = Product.id LIMIT 10")
                .unwrap();
        assert_eq!(q.joins.len(), 1);
        assert_eq!(q.joins[0].target_model, "Review");
    }

    #[test]
    fn test_parse_star() {
        let q = parse("SELECT * FROM Product LIMIT 10").unwrap();
        assert_eq!(q.select.len(), 1);
        assert_eq!(q.select[0].name, "*");
    }

    #[test]
    fn test_parse_temporal_field() {
        let q = parse("SELECT name, price_30d_ago, price_trend FROM Product LIMIT 10").unwrap();
        assert_eq!(q.select.len(), 3);

        assert!(q.select[1].temporal_func.is_some());
        assert!(matches!(
            q.select[1].temporal_func,
            Some(TemporalFunc::ValueAgo(30))
        ));

        assert!(q.select[2].temporal_func.is_some());
        assert!(matches!(
            q.select[2].temporal_func,
            Some(TemporalFunc::Trend)
        ));
    }

    #[test]
    fn test_parse_string_comparison() {
        let q = parse("SELECT name FROM Product WHERE availability = 'in_stock' LIMIT 10").unwrap();
        if let Some(WhereExpr::Comparison { value, .. }) = &q.where_clause {
            assert!(matches!(value, WqlValue::String(_)));
        }
    }

    #[test]
    fn test_parse_error_missing_from() {
        let result = parse("SELECT name LIMIT 10");
        assert!(result.is_err());
    }

    #[test]
    fn test_tokenize_error_on_unterminated_string() {
        let result = parse("SELECT name FROM Product WHERE x = 'unterminated");
        assert!(result.is_err());
    }

    // ── v4 Test Suite: Phase 4A — Parser ──

    #[test]
    fn test_v4_parse_all_valid_queries() {
        let queries = vec![
            "SELECT name, price FROM Product LIMIT 10",
            "SELECT name, price FROM Product WHERE price < 200",
            "SELECT name, price, rating FROM Product WHERE price < 200 AND rating > 4.0 ORDER BY price ASC LIMIT 5",
            "SELECT name, price FROM Product ACROSS amazon.com, bestbuy.com LIMIT 10",
            "SELECT name, price, price_30d_ago FROM Product WHERE price < 500 LIMIT 10",
        ];

        for q in &queries {
            let result = parse(q);
            assert!(
                result.is_ok(),
                "should parse: {q}\nerror: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn test_v4_parse_malformed_queries_gracefully() {
        // Spec: WQL must handle malformed queries gracefully (error message, not crash)
        let bad_queries = vec![
            "",
            "SELECT",
            "FROM Product",
            "SELECT FROM",
            "SELECT name Product",
            "SELECT name FROM LIMIT",
            "SELECT name FROM Product WHERE",
            "SELECT name FROM Product ORDER BY",
            "SELECTT name FROM Product",
        ];

        for q in &bad_queries {
            let result = parse(q);
            assert!(result.is_err(), "malformed query should error: {q}");
            // Should produce a meaningful error, not panic
            let err = result.unwrap_err();
            assert!(
                !err.to_string().is_empty(),
                "error message should not be empty for: {q}"
            );
        }
    }

    #[test]
    fn test_v4_parse_temporal_predicted() {
        let q = parse("SELECT name, predicted_price_7d FROM Product LIMIT 5").unwrap();
        let pred_field = &q.select[1];
        assert!(pred_field.temporal_func.is_some());
        assert!(matches!(
            pred_field.temporal_func,
            Some(TemporalFunc::Predicted(7))
        ));
    }

    #[test]
    fn test_v4_parse_complex_where() {
        let q = parse(
            "SELECT name, price FROM Product WHERE price < 200 AND rating > 4.0 ORDER BY price ASC LIMIT 5"
        ).unwrap();

        assert!(q.where_clause.is_some());
        assert!(q.order_by.is_some());
        assert_eq!(q.limit, Some(5));

        if let Some(order_fields) = &q.order_by {
            assert!(!order_fields.is_empty());
            assert_eq!(order_fields[0].field, "price");
            assert!(order_fields[0].ascending);
        }
    }

    #[test]
    fn test_v4_parse_across_multiple_domains() {
        let q =
            parse("SELECT name FROM Product ACROSS amazon.com, bestbuy.com, walmart.com LIMIT 20")
                .unwrap();

        assert!(q.across.is_some());
        let domains = q.across.unwrap();
        assert_eq!(domains.len(), 3);
        assert_eq!(domains[0], "amazon.com");
        assert_eq!(domains[1], "bestbuy.com");
        assert_eq!(domains[2], "walmart.com");
    }

    #[test]
    fn test_v4_parse_preserves_field_names() {
        let q = parse("SELECT name, price, rating, availability FROM Product LIMIT 10").unwrap();
        assert_eq!(q.select.len(), 4);
        assert_eq!(q.select[0].name, "name");
        assert_eq!(q.select[1].name, "price");
        assert_eq!(q.select[2].name, "rating");
        assert_eq!(q.select[3].name, "availability");
    }
}
