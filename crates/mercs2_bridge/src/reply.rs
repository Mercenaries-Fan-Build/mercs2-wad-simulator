//! A chunk's result text, as a value.
//!
//! The ASI renders a result as `[ok]\t<v1>\t<v2>…`, or `[runtime] …`, `[compile] …`, `[bridge] …` when the
//! chunk did not finish (`mercs2_formats::lua_exec` is the format's source). [`parse_reply`] reads that
//! text from [`crate::Bridge::eval`] into a [`Reply`].

/// One value of an `[ok]` reply, as `mercs2_formats::lua_exec::format_value` renders it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table,
    Function,
    /// Anything else (`<tt=… val=…>`, unreadable strings, or a token a tab inside a string split).
    Other(String),
}

/// A whole REPL reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok(Vec<Value>),
    Runtime(String),
    Compile(String),
    Bridge(String),
    Unrecognized(String),
}

fn is_number_text(t: &str) -> bool {
    t.bytes().any(|b| b.is_ascii_digit()) && t.bytes().all(|b| b.is_ascii_digit() || b"+-.e".contains(&b))
}

/// One tab-separated token of an `[ok]` reply.
pub fn parse_value(tok: &str) -> Value {
    match tok {
        "nil" => Value::Nil,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "<table>" => Value::Table,
        "<function>" => Value::Function,
        "1.#INF" => Value::Num(f64::INFINITY),
        "-1.#INF" => Value::Num(f64::NEG_INFINITY),
        "1.#QNAN" | "-1.#IND" => Value::Num(f64::NAN),
        _ if tok.len() >= 2 && tok.starts_with('"') && tok.ends_with('"') => {
            Value::Str(tok[1..tok.len() - 1].to_string())
        }
        _ if is_number_text(tok) => tok.parse::<f64>().map(Value::Num).unwrap_or_else(|_| Value::Other(tok.to_string())),
        _ => Value::Other(tok.to_string()),
    }
}

/// Parse the text [`crate::Bridge::eval`] returned.
pub fn parse_reply(text: &str) -> Reply {
    if text == "[ok]" {
        Reply::Ok(Vec::new())
    } else if let Some(rest) = text.strip_prefix("[ok]\t") {
        Reply::Ok(rest.split('\t').map(parse_value).collect())
    } else if let Some(rest) = text.strip_prefix("[runtime]") {
        Reply::Runtime(rest.trim_start().to_string())
    } else if let Some(rest) = text.strip_prefix("[compile]") {
        Reply::Compile(rest.trim_start().to_string())
    } else if let Some(rest) = text.strip_prefix("[bridge]") {
        Reply::Bridge(rest.trim_start().to_string())
    } else {
        Reply::Unrecognized(text.to_string())
    }
}
