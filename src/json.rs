//! A minimal recursive-descent JSON parser. `usage-exact.json` has a fixed,
//! tiny schema, so this covers just what a real document can contain without
//! pulling in a serialization framework.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(m) => m.get(key),
            _ => None,
        }
    }

    /// Walk a dotted path, e.g. `metrics.session.percent_used`.
    pub fn path(&self, path: &str) -> Option<&Value> {
        path.split('.').try_fold(self, |v, k| v.get(k))
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse(input: &str) -> Result<Value, String> {
    let bytes = input.as_bytes();
    let mut p = Parser { b: bytes, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != bytes.len() {
        return Err(format!("trailing data at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.b[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek().ok_or("unexpected end of input")? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Value::Str(self.string()?)),
            b't' if self.eat(b"true") => Ok(Value::Bool(true)),
            b'f' if self.eat(b"false") => Ok(Value::Bool(false)),
            b'n' if self.eat(b"null") => Ok(Value::Null),
            _ => self.number(),
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.i += 1; // '{'
        let mut map = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(map));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(format!("expected ':' at byte {}", self.i));
            }
            self.i += 1;
            self.ws();
            map.insert(key, self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(map));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.i)),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.i += 1; // '['
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(out));
        }
        loop {
            self.ws();
            out.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(out));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.i)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err(format!("expected string at byte {}", self.i));
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self.b.get(self.i).ok_or("unterminated string")?;
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = *self.b.get(self.i).ok_or("unterminated escape")?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .b
                                .get(self.i..self.i + 4)
                                .ok_or("truncated \\u escape")?;
                            let cp = u32::from_str_radix(
                                std::str::from_utf8(hex).map_err(|e| e.to_string())?,
                                16,
                            )
                            .map_err(|e| e.to_string())?;
                            self.i += 4;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(format!("bad escape \\{}", e as char)),
                    }
                }
                _ => {
                    // Copy the raw UTF-8 byte; multi-byte sequences pass through.
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    let slice = self
                        .b
                        .get(start..start + len)
                        .ok_or("truncated UTF-8 sequence")?;
                    out.push_str(std::str::from_utf8(slice).map_err(|e| e.to_string())?);
                    self.i = start + len;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-') {
                self.i += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.b[start..self.i])
            .map_err(|e| e.to_string())?
            .parse::<f64>()
            .map(Value::Num)
            .map_err(|_| format!("invalid number at byte {start}"))
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Serialize a string as a JSON string literal, quotes included.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn to_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            // Emit integral values without a trailing ".0" so round-tripping
            // the credentials file does not rewrite expiresAt into a float.
            if n.fract() == 0.0 && n.abs() < 9e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::Str(s) => quote(s),
        Value::Arr(a) => {
            let items: Vec<String> = a.iter().map(to_string).collect();
            format!("[{}]", items.join(","))
        }
        Value::Obj(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}:{}", quote(k), to_string(v)))
                .collect();
            format!("{{{}}}", items.join(","))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_usage_document_shape() {
        let v = parse(
            r#"{"timestamp":"2026-08-24T00:06:25Z","metrics":{"session":{"percent_used":12.5,
               "resets_at":"2026-08-24T00:10:00.143043+00:00"},"week_sonnet":null}}"#,
        )
        .unwrap();
        assert_eq!(
            v.path("metrics.session.percent_used").unwrap().as_f64(),
            Some(12.5)
        );
        assert_eq!(v.path("metrics.week_sonnet"), Some(&Value::Null));
        assert_eq!(v.path("metrics.session.nope"), None);
    }

    #[test]
    fn handles_escapes_and_unicode() {
        let v = parse(r#"{"a":"x\"y\\z\n","b":"\u00e9","c":"héllo"}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_str(), Some("x\"y\\z\n"));
        assert_eq!(v.get("b").unwrap().as_str(), Some("é"));
        assert_eq!(v.get("c").unwrap().as_str(), Some("héllo"));
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in [
            r#"{"a":}"#,
            r#"{"a" 1}"#,
            r#"{"a":1"#,
            r#"{}extra"#,
            r#""unterminated"#,
        ] {
            assert!(parse(bad).is_err(), "should have rejected {bad}");
        }
    }

    #[test]
    fn round_trips_without_turning_integers_into_floats() {
        // expiresAt is a large integer; rewriting it as 1.7e12 would corrupt
        // the credentials file we do not own.
        let v = parse(r#"{"expiresAt":1756000000000,"ratio":0.5}"#).unwrap();
        let out = to_string(&v);
        assert!(out.contains("\"expiresAt\":1756000000000"), "got {out}");
        assert!(out.contains("\"ratio\":0.5"), "got {out}");
        assert_eq!(parse(&out).unwrap(), v);
    }

    #[test]
    fn quotes_control_characters() {
        assert_eq!(quote("a\tb"), "\"a\\tb\"");
        assert_eq!(quote("\u{1}"), "\"\\u0001\"");
    }
}
