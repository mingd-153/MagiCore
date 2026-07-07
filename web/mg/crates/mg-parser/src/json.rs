use std::str;

#[derive(Debug, Default, Clone)]
pub struct Manifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub dependencies: Vec<Dep>,
    pub dev_dependencies: Vec<Dep>,
    pub peer_dependencies: Vec<Dep>,
}

impl Manifest {
    pub fn is_complete(&self) -> bool {
        self.name.is_some() && self.version.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("expected string")]
    ExpectedString,
    #[error("invalid UTF-8")]
    InvalidUtf8,
    #[error("unexpected token")]
    UnexpectedToken,
    #[error("expected '{{'")]
    ExpectedObject,
    #[error("expected ':'")]
    ExpectedColon,
    #[error("expected ',' or '}}'")]
    ExpectedCommaOrClose,
    #[error("escaped strings not supported in zero-copy mode")]
    EscapedString,
}

pub struct ZeroCopyParser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> ZeroCopyParser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    pub fn parse_manifest(&mut self) -> Result<Manifest, ParseError> {
        let mut manifest = Manifest::default();

        self.skip_whitespace();
        self.expect(b'{')?;

        loop {
            self.skip_whitespace();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }

            let key = self.read_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();

            match key.as_str() {
                "name" => {
                    manifest.name = Some(self.read_string()?);
                }
                "version" => {
                    manifest.version = Some(self.read_string()?);
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
                _ => {
                    self.skip_value()?;
                }
            }

            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.pos += 1;
            }
        }

        Ok(manifest)
    }

    fn read_string(&mut self) -> Result<String, ParseError> {
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
                    let s = str::from_utf8(slice).map_err(|_| ParseError::InvalidUtf8)?;
                    return Ok(s.to_string());
                }
                Some(b'\\') => {
                    return Err(ParseError::EscapedString);
                }
                Some(_) => self.pos += 1,
                None => return Err(ParseError::UnexpectedEof),
            }
        }
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        match self.peek() {
                Some(b'"') => {
                    self.read_string()?;
                    Ok(())
                }
            Some(b'{') => self.skip_object(),
            Some(b'[') => self.skip_array(),
            Some(b't') | Some(b'f') => self.skip_literal(),
            Some(b'n') => self.skip_null(),
            Some(c) if c.is_ascii_digit() || c == b'-' => self.skip_number(),
            _ => Err(ParseError::UnexpectedToken),
        }
    }

    fn parse_dep_object(&mut self) -> Result<Vec<Dep>, ParseError> {
        let mut deps = Vec::new();
        self.skip_whitespace();
        self.expect(b'{')?;

        loop {
            self.skip_whitespace();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }

            let name = self.read_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let version = self.read_string()?;

            deps.push(Dep { name, version });

            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.pos += 1;
            }
        }

        Ok(deps)
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
            Err(ParseError::ExpectedObject) // generic fallback
        }
    }

    fn skip_object(&mut self) -> Result<(), ParseError> {
        self.expect(b'{')?;
        let mut depth = 1u32;
        while depth > 0 {
            match self.peek() {
                Some(b'{') => depth += 1,
                Some(b'}') => depth -= 1,
                Some(b'"') => {
                    self.pos += 1;
                    while self.input.get(self.pos) != Some(&b'"') {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                Some(_) => {}
                None => return Err(ParseError::UnexpectedEof),
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn skip_array(&mut self) -> Result<(), ParseError> {
        self.expect(b'[')?;
        let mut depth = 1u32;
        while depth > 0 {
            match self.peek() {
                Some(b'[') => depth += 1,
                Some(b']') => depth -= 1,
                Some(b'"') => {
                    self.pos += 1;
                    while self.input.get(self.pos) != Some(&b'"') {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                Some(_) => {}
                None => return Err(ParseError::UnexpectedEof),
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn skip_literal(&mut self) -> Result<(), ParseError> {
        while let Some(&c) = self.input.get(self.pos) {
            if c.is_ascii_alphabetic() {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_null(&mut self) -> Result<(), ParseError> {
        self.skip_literal()
    }

    fn skip_number(&mut self) -> Result<(), ParseError> {
        while let Some(&c) = self.input.get(self.pos) {
            if c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'e' || c == b'E' || c == b'+'
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_version() {
        let input = br#"{"name": "lodash", "version": "4.17.21"}"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("lodash"));
        assert_eq!(manifest.version.as_deref(), Some("4.17.21"));
    }

    #[test]
    fn test_parse_with_deps() {
        let input = br#"{
            "name": "mypkg",
            "version": "1.0.0",
            "dependencies": {
                "react": "^18.0.0",
                "lodash": "^4.17.0"
            }
        }"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("mypkg"));
        assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies[0].name, "react");
        assert_eq!(manifest.dependencies[1].version, "^4.17.0");
    }

    #[test]
    fn test_early_termination() {
        let input = br#"{
            "name": "fast",
            "version": "1.0.0",
            "unnecessaryField": {"deep": {"nested": [1,2,3]}},
            "anotherField": "should not parse this"
        }"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.name.as_deref(), Some("fast"));
        assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
        assert_eq!(manifest.dependencies.len(), 0);
    }

    #[test]
    fn test_empty_manifest() {
        let input = br#"{}"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert!(manifest.name.is_none());
        assert!(manifest.version.is_none());
    }

    #[test]
    fn test_dev_deps() {
        let input = br#"{
            "name": "pkg",
            "version": "1.0.0",
            "devDependencies": {
                "jest": "^29.0.0"
            }
        }"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.dev_dependencies.len(), 1);
        assert_eq!(manifest.dev_dependencies[0].name, "jest");
    }

    #[test]
    fn test_peer_deps() {
        let input = br#"{
            "name": "pkg",
            "version": "1.0.0",
            "peerDependencies": {
                "react": "^18.0.0"
            }
        }"#;
        let mut parser = ZeroCopyParser::new(input);
        let manifest = parser.parse_manifest().unwrap();
        assert_eq!(manifest.peer_dependencies.len(), 1);
    }

    #[test]
    fn test_parse_error_unexpected_eof() {
        let input = br#"{"name":"#;
        let mut parser = ZeroCopyParser::new(input);
        let result = parser.parse_manifest();
        assert!(result.is_err());
    }
}
