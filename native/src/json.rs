//! Just enough JSON for the Pi server's events and state: parse a value, look up keys, and
//! quote a string for the small objects the player sends back.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map_or(&Json::Null, |(_, v)| v),
            _ => &Json::Null,
        }
    }

    pub fn str(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }

    pub fn num(&self) -> Option<f64> {
        if let Json::Num(n) = self { Some(*n) } else { None }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Json::Null => false,
            Json::Bool(b) => *b,
            Json::Num(n) => *n != 0.0,
            Json::Str(s) => !s.is_empty(),
            _ => true,
        }
    }
}

pub fn parse(s: &str) -> Option<Json> {
    let mut p = Parser { b: s.as_bytes(), i: 0 };
    let v = p.value()?;
    p.ws();
    (p.i == p.b.len()).then_some(v)
}

/// `s` as a JSON string literal, quotes included.
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

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.b[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.eat("}") {
                    return Some(Json::Obj(kv));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return None;
                    }
                    kv.push((k, self.value()?));
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    return self.eat("}").then_some(Json::Obj(kv));
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.eat("]") {
                    return Some(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    self.ws();
                    if self.eat(",") {
                        continue;
                    }
                    return self.eat("]").then_some(Json::Arr(a));
                }
            }
            b'"' => self.string().map(Json::Str),
            b't' => self.eat("true").then_some(Json::Bool(true)),
            b'f' => self.eat("false").then_some(Json::Bool(false)),
            b'n' => self.eat("null").then_some(Json::Null),
            _ => {
                let start = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                std::str::from_utf8(&self.b[start..self.i]).ok()?.parse().ok().map(Json::Num)
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(h, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat("\"") {
            return None;
        }
        let mut out = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut u = self.hex4()?;
                            // a surrogate pair: 🎉
                            if (0xD800..0xDC00).contains(&u) && self.eat("\\u") {
                                let lo = self.hex4()?;
                                u = 0x10000 + ((u - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            char::from_u32(u).unwrap_or('\u{FFFD}')
                        }
                        _ => return None,
                    };
                    let mut buf = [0; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                c => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_events() {
        let v = parse(r#"{"cmd": "demo", "arg": 42}"#).unwrap();
        assert_eq!(v.get("cmd").str(), Some("demo"));
        assert_eq!(v.get("arg").num(), Some(42.0));
        let v = parse(r#"{"cmd": "nowplaying", "arg": {"source": "airplay", "artist": "Roxette", "title": "Joyride åäö 🎉", "playing": true}}"#).unwrap();
        let a = v.get("arg");
        assert_eq!(a.get("artist").str(), Some("Roxette"));
        assert_eq!(a.get("title").str(), Some("Joyride åäö 🎉"));
        assert!(a.get("playing").truthy());
        assert_eq!(v.get("missing"), &Json::Null);
        let v = parse(r#"{"urls": {"mdns": "http://ufoscene.local/", "ip": null}, "list": [1, -2.5e1, "x"], "e": {}}"#).unwrap();
        assert_eq!(v.get("urls").get("ip"), &Json::Null);
        assert_eq!(v.get("list"), &Json::Arr(vec![Json::Num(1.0), Json::Num(-25.0), Json::Str("x".into())]));
    }

    #[test]
    fn rejects_broken() {
        for s in ["", "{", r#"{"a":}"#, r#"{"a":1,}"#, "[1 2]", r#""abc"#, "tru", "{} x"] {
            assert_eq!(parse(s), None, "{s}");
        }
    }

    #[test]
    fn quote_round_trip() {
        let s = "a \"b\" \\ c\nÅ\u{1}";
        assert_eq!(parse(&quote(s)), Some(Json::Str(s.into())));
    }
}
