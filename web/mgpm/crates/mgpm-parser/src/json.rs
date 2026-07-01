use std::borrow::Cow;

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParseError {
    #[error("expected string")]
    ExpectedString,
    #[error("expected '{0}'")]
    ExpectedChar(char),
    #[error("unexpected token")]
    UnexpectedToken,
    #[error("unexpected EOF")]
    UnexpectedEof,
    #[error("invalid UTF-8")]
    InvalidUtf8,
}

impl From<std::str::Utf8Error> for ParseError {
    fn from(_: std::str::Utf8Error) -> Self {
        Self::InvalidUtf8
    }
}

#[derive(Debug, Default, Clone)]
pub struct Manifest<'a> {
    pub name: Option<Cow<'a, str>>,
    pub version: Option<Cow<'a, str>>,
    pub dependencies: Vec<Dep<'a>>,
    pub dev_dependencies: Vec<Dep<'a>>,
    pub peer_dependencies: Vec<Dep<'a>>,
    pub dist: Option<DistInfo<'a>>,
}

impl<'a> Manifest<'a> {
    pub fn is_complete(&self) -> bool {
        self.name.is_some()
            && self.version.is_some()
            && !self.dependencies.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct Dep<'a> {
    pub name: Cow<'a, str>,
    pub version: Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub struct DistInfo<'a> {
    pub tarball: Option<Cow<'a, str>>,
    pub integrity: Option<Cow<'a, str>>,
}

pub struct ZeroCopyParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> ZeroCopyParser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    pub fn parse_manifest(&mut self) -> Result<Manifest<'a>, ParseError> {
        let mut manifest = Manifest::default();

        self.skip_whitespace();
        self.expect(b'{')?;

        loop {
            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.pos += 1;
                self.skip_whitespace();
            }
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }

            let key = self.read_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();

            match key.as_ref() {
                "name" => {
                    manifest.name = Some(self.read_string_cow()?);
                }
                "version" => {
                    manifest.version = Some(self.read_string_cow()?);
                }
                "dependencies" => {
                    manifest.dependencies = self.parse_dep_object()?;
                }
                "devDependencies" => {
                    manifest.dev_dependencies = self.parse_dep_object()?;
                }
                "peerDependencies" => {
                    manifest.peer_dependencies = self.parse_dep_object()?;
                }
                "dist" => {
                    manifest.dist = Some(self.parse_dist_object()?);
                }
                _ => {
                    self.skip_value()?;
                }
            }
        }

        Ok(manifest)
    }

    fn read_string(&mut self) -> Result<Cow<'a, str>, ParseError> {
        self.read_string_cow()
    }

    fn read_string_cow(&mut self) -> Result<Cow<'a, str>, ParseError> {
        self.skip_whitespace();
        if self.peek() != Some(b'"') {
            return Err(ParseError::ExpectedString);
        }
        self.pos += 1;

        let start = self.pos;
        loop {
            match self.input.get(self.pos) {
                Some(b'"') => {
                    let slice = &self.input[start..self.pos];
                    self.pos += 1;
                    return Ok(Cow::Borrowed(std::str::from_utf8(slice)?));
                }
                Some(b'\\') => {
                    return self.read_escaped_string(start).map(Cow::Owned);
                }
                Some(_) => self.pos += 1,
                None => return Err(ParseError::UnexpectedEof),
            }
        }
    }

    fn read_escaped_string(&mut self, start: usize) -> Result<String, ParseError> {
        let mut buf = String::with_capacity(64);
        buf.push_str(
            std::str::from_utf8(&self.input[start..self.pos])
                .map_err(|_| ParseError::InvalidUtf8)?,
        );

        loop {
            match self.input.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(buf);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.input.get(self.pos) {
                        Some(b'"') => buf.push('"'),
                        Some(b'\\') => buf.push('\\'),
                        Some(b'/') => buf.push('/'),
                        Some(b'n') => buf.push('\n'),
                        Some(b'r') => buf.push('\r'),
                        Some(b't') => buf.push('\t'),
                        Some(b'b') => buf.push('\u{0008}'),
                        Some(b'f') => buf.push('\u{000C}'),
                        Some(b'u') => {
                            let hex = self.read_unicode_escape()?;
                            buf.push(hex);
                            continue;
                        }
                        _ => return Err(ParseError::UnexpectedToken),
                    }
                    self.pos += 1;
                }
                Some(&c) if c < 128 => {
                    buf.push(c as char);
                    self.pos += 1;
                }
                Some(_) => {
                    let s = std::str::from_utf8(&self.input[self.pos..])
                        .map_err(|_| ParseError::InvalidUtf8)?;
                    let ch = s.chars().next().ok_or(ParseError::UnexpectedEof)?;
                    buf.push(ch);
                    self.pos += ch.len_utf8();
                }
                None => return Err(ParseError::UnexpectedEof),
            }
        }
    }

    fn read_unicode_escape(&mut self) -> Result<char, ParseError> {
        let hex_start = self.pos + 1;
        let hex_end = hex_start + 4;
        if hex_end > self.input.len() {
            return Err(ParseError::UnexpectedEof);
        }
        let hex_str = std::str::from_utf8(&self.input[hex_start..hex_end])
            .map_err(|_| ParseError::InvalidUtf8)?;
        let code = u32::from_str_radix(hex_str, 16)
            .map_err(|_| ParseError::UnexpectedToken)?;
        self.pos = hex_end;
        char::from_u32(code).ok_or(ParseError::UnexpectedToken)
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'"') => { self.read_string_cow()?; Ok(()) }
            Some(b'{') => self.skip_object(),
            Some(b'[') => self.skip_array(),
            Some(b't') | Some(b'f') => self.skip_literal(),
            Some(b'n') => self.skip_null(),
            Some(c) if c.is_ascii_digit() || c == b'-' => self.skip_number(),
            _ => Err(ParseError::UnexpectedToken),
        }
    }

    fn skip_object(&mut self) -> Result<(), ParseError> {
        self.expect(b'{')?;
        let mut depth = 1usize;
        while depth > 0 {
            match self.input.get(self.pos) {
                Some(b'{') => { depth += 1; self.pos += 1; }
                Some(b'}') => { depth -= 1; self.pos += 1; }
                Some(b'"') => { self.read_string_cow()?; }
                Some(_) => { self.pos += 1; }
                None => return Err(ParseError::UnexpectedEof),
            }
        }
        Ok(())
    }

    fn skip_array(&mut self) -> Result<(), ParseError> {
        self.expect(b'[')?;
        let mut depth = 1usize;
        while depth > 0 {
            match self.input.get(self.pos) {
                Some(b'[') => { depth += 1; self.pos += 1; }
                Some(b']') => { depth -= 1; self.pos += 1; }
                Some(b'"') => { self.read_string_cow()?; }
                Some(_) => { self.pos += 1; }
                None => return Err(ParseError::UnexpectedEof),
            }
        }
        Ok(())
    }

    fn skip_literal(&mut self) -> Result<(), ParseError> {
        while let Some(&c) = self.input.get(self.pos) {
            if c.is_ascii_alphabetic() {
                self.pos += 1;
            } else {
                return Ok(());
            }
        }
        Err(ParseError::UnexpectedEof)
    }

    fn skip_null(&mut self) -> Result<(), ParseError> {
        self.skip_literal()
    }

    fn skip_number(&mut self) -> Result<(), ParseError> {
        while let Some(&c) = self.input.get(self.pos) {
            if c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'.'
                || c == b'e' || c == b'E'
            {
                self.pos += 1;
            } else {
                return Ok(());
            }
        }
        Err(ParseError::UnexpectedEof)
    }

    fn parse_dep_object(&mut self) -> Result<Vec<Dep<'a>>, ParseError> {
        let mut deps = Vec::new();
        self.skip_whitespace();
        self.expect(b'{')?;

        loop {
            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.pos += 1;
                self.skip_whitespace();
            }
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }

            let name = self.read_string_cow()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let version = self.read_string_cow()?;

            deps.push(Dep { name, version });
        }

        Ok(deps)
    }

    fn parse_dist_object(&mut self) -> Result<DistInfo<'a>, ParseError> {
        let mut dist = DistInfo {
            tarball: None,
            integrity: None,
        };

        self.skip_whitespace();
        self.expect(b'{')?;

        loop {
            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.pos += 1;
                self.skip_whitespace();
            }
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }

            let key = self.read_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();

            match key.as_ref() {
                "tarball" => dist.tarball = Some(self.read_string_cow()?),
                "integrity" => dist.integrity = Some(self.read_string_cow()?),
                _ => { self.skip_value()?; }
            }
        }

        Ok(dist)
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.input.get(self.pos) {
            if c == b' ' || c == b'\n' || c == b'\r' || c == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn expect(&mut self, expected: u8) -> Result<(), ParseError> {
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError::ExpectedChar(expected as char))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_manifest() {
        let json = br#"{
            "name": "lodash",
            "version": "4.17.21",
            "dependencies": {},
            "main": "lodash.js"
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();

        assert_eq!(manifest.name.as_deref(), Some("lodash"));
        assert_eq!(manifest.version.as_deref(), Some("4.17.21"));
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_parse_with_deps() {
        let json = br#"{
            "name": "express",
            "version": "4.18.2",
            "dependencies": {
                "accepts": "~1.3.8",
                "array-flatten": "1.1.1",
                "body-parser": "1.20.1",
                "content-disposition": "0.5.4"
            }
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();

        assert_eq!(manifest.name.as_deref(), Some("express"));
        assert_eq!(manifest.dependencies.len(), 4);
        assert_eq!(manifest.dependencies[0].name.as_ref(), "accepts");
        assert_eq!(manifest.dependencies[0].version.as_ref(), "~1.3.8");
    }

    #[test]
    fn test_skip_unknown_fields_with_script() {
        let json = br#"{
            "name": "react",
            "version": "18.3.1",
            "dependencies": {"loose-envify": "^1.1.0"},
            "bundledDependencies": ["loose-envify"],
            "scripts": {"build": "..."},
            "engines": {"node": ">=0.10.0"},
            "repository": {"type": "git", "url": "..."},
            "license": "MIT"
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();

        assert!(manifest.is_complete());
        assert_eq!(manifest.dependencies.len(), 1);
    }

    #[test]
    fn test_parse_dev_deps() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "dependencies": {"express": "^4.0.0"},
            "devDependencies": {
                "mocha": "^10.0.0",
                "chai": "^4.0.0"
            }
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();

        assert_eq!(manifest.name.as_deref(), Some("test"));
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dev_dependencies.len(), 2);
        assert_eq!(manifest.dev_dependencies[0].name.as_ref(), "mocha");
    }

    #[test]
    fn test_parse_dist() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "dist": {
                "tarball": "https://registry.npmjs.org/test/-/test-1.0.0.tgz",
                "integrity": "sha512-abc123",
                "fileCount": 42
            },
            "dependencies": {"foo": "^1.0.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();

        assert!(manifest.dist.is_some());
        let dist = manifest.dist.as_ref().unwrap();
        assert_eq!(
            dist.tarball.as_deref(),
            Some("https://registry.npmjs.org/test/-/test-1.0.0.tgz")
        );
        assert_eq!(dist.integrity.as_deref(), Some("sha512-abc123"));
    }

    #[test]
    fn test_malformed_json() {
        let cases: &[&[u8]] = &[
            br#"{"name": "test"#,
            br#"{name: "test"}"#,
            br#"{"name": "test",,}"#,
            br#"{"name": test}"#,
        ];

        for case in cases {
            let mut parser = ZeroCopyParser::new(case);
            assert!(parser.parse_manifest().is_err());
        }
    }

    #[test]
    fn test_empty_object() {
        let json = b"{}";
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert!(manifest.name.is_none());
        assert!(manifest.version.is_none());
    }

    #[test]
    fn test_skip_unknown_fields() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "scripts": {"build": "tsc", "test": "jest"},
            "repository": {"type": "git", "url": "https://github.com/test/repo"},
            "engines": {"node": ">=14"},
            "dependencies": {"foo": "^1.0.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("test"));
        assert_eq!(manifest.dependencies.len(), 1);
    }

    #[test]
    fn test_unicode_escape() {
        let json = br#"{"name": "\u0041\u0042", "version": "1.0.0", "dependencies": {"a": "^1.0.0"}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("AB"));
    }

    #[test]
    fn test_scoped_package_name() {
        let json = br#"{
            "name": "@types/node",
            "version": "20.0.0",
            "dependencies": {"undici-types": "~5.0.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("@types/node"));
    }

    #[test]
    fn test_escaped_newline() {
        let json = br#"{"name": "line1\nline2", "version": "1.0.0", "dependencies": {"a": "1.0.0"}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("line1\nline2"));
    }

    #[test]
    fn test_escaped_string_non_ascii() {
        let json = br#"{"name": "h\u00e9llo\nw\u00f6rld", "version": "1.0.0", "dependencies": {"a": "1.0.0"}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("héllo\nwörld"));
    }

    #[test]
    fn test_escaped_string_non_ascii_raw_after_escape() {
        let json = "{\"name\": \"hello\\ncaf\u{00e9}\", \"version\": \"1.0.0\", \"dependencies\": {\"a\": \"1.0.0\"}}".as_bytes();
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("hello\ncafé"));
    }

    #[test]
    fn test_nested_skip_array() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "keywords": ["a", "b", {"nested": ["deep"]}],
            "dependencies": {"foo": "^1.0.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("test"));
        assert_eq!(manifest.dependencies.len(), 1);
    }

    #[test]
    fn test_numbers_and_bools() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "deprecated": false,
            "gitHead": "abc123def456",
            "hasInstallScript": true,
            "_hasShrinkwrap": false,
            "dependencies": {"bar": "2.0.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("test"));
        assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
        assert_eq!(manifest.dependencies.len(), 1);
    }

    #[test]
    fn test_no_deps_parse_all() {
        let json = br#"{
            "name": "solo-pkg",
            "version": "0.0.1",
            "description": "a package with no deps at all"
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("solo-pkg"));
        assert_eq!(manifest.version.as_deref(), Some("0.0.1"));
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_zero_copy_borrowed() {
        let input = br#"{"name": "hello", "version": "1.0.0", "dependencies": {"x": "0.0.1"}}"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();

        let name_ptr = manifest.name.as_ref().unwrap().as_ref().as_ptr();
        let input_name_pos = input
            .windows(b"hello".len())
            .position(|w| w == b"hello")
            .unwrap();
        assert_eq!(
            name_ptr as usize,
            input[input_name_pos..].as_ptr() as usize,
            "should point into input buffer"
        );
    }

    #[test]
    fn test_escaped_produces_owned() {
        let json = br#"{"name": "test\nname", "version": "1.0.0", "dependencies": {"x": "1.0.0"}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert!(matches!(manifest.name.as_ref().unwrap(), Cow::Owned(_)));
    }

    #[test]
    fn test_unescaped_produces_borrowed() {
        let json = br#"{"name": "plain-name", "version": "1.0.0", "dependencies": {"x": "1.0.0"}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert!(matches!(manifest.name.as_ref().unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn test_peer_dependencies() {
        let json = br#"{
            "name": "test",
            "version": "1.0.0",
            "peerDependencies": {"react": "^18.0.0"},
            "dependencies": {"scheduler": "^0.23.0"}
        }"#;

        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.peer_dependencies.len(), 1);
        assert_eq!(manifest.peer_dependencies[0].name.as_ref(), "react");
    }

    #[test]
    fn test_empty_deps_object() {
        let json = br#"{"name": "test", "version": "1.0.0", "dependencies": {}}"#;
        let mut parser = ZeroCopyParser::new(json);
        let manifest = parser.parse_manifest().unwrap();
        assert!(manifest.dependencies.is_empty());
        // With empty deps, is_complete is false (needs non-empty deps)
        assert!(!manifest.is_complete());
    }
}
