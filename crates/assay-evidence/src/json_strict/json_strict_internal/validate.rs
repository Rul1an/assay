use crate::json_strict::dupkeys::ObjectKeyTracker;
use crate::json_strict::errors::StrictJsonError;
use std::str::CharIndices;

pub(crate) struct JsonValidator<'a> {
    chars: std::iter::Peekable<CharIndices<'a>>,
    key_tracker: ObjectKeyTracker,
    current_path: String,
    depth: usize,
    _phantom: std::marker::PhantomData<&'a str>,
}

impl<'a> JsonValidator<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            key_tracker: ObjectKeyTracker::new(),
            current_path: String::new(),
            depth: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    pub(crate) fn validate(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();
        self.validate_value()?;
        Ok(())
    }

    fn validate_value(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('{') => self.validate_object(),
            Some('[') => self.validate_array(),
            Some('"') => self.validate_string().map(|_| ()),
            Some(c) if c == '-' || c.is_ascii_digit() => self.validate_number(),
            Some('t') | Some('f') => self.validate_bool(),
            Some('n') => self.validate_null(),
            Some(c) => Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>(&format!("unexpected char '{}'", c)).unwrap_err(),
            )),
            None => Ok(()),
        }
    }

    fn validate_object(&mut self) -> Result<(), StrictJsonError> {
        let _max_keys = super::limits::MAX_KEYS_PER_OBJECT;
        self.expect_char('{')?;
        self.skip_whitespace();

        self.depth += 1;
        if self.depth > super::limits::MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep { depth: self.depth });
        }

        let path = self.current_path.clone();
        self.key_tracker.enter_object(path);

        if self.peek_char() == Some('}') {
            self.next_char();
            self.key_tracker.exit_object();
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.skip_whitespace();
            let key = self.validate_string()?;
            self.key_tracker.push_key(key.clone())?;

            self.skip_whitespace();
            self.expect_char(':')?;

            let old_path = self.current_path.clone();
            self.current_path = if self.current_path.is_empty() {
                format!("/{}", key)
            } else {
                format!("{}/{}", self.current_path, key)
            };

            self.validate_value()?;
            self.current_path = old_path;
            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                }
                Some('}') => {
                    self.next_char();
                    self.key_tracker.exit_object();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("expected ',' or '}'").unwrap_err(),
                    ));
                }
            }
        }
    }

    fn validate_array(&mut self) -> Result<(), StrictJsonError> {
        self.expect_char('[')?;
        self.skip_whitespace();

        self.depth += 1;
        if self.depth > super::limits::MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep { depth: self.depth });
        }

        if self.peek_char() == Some(']') {
            self.next_char();
            self.depth -= 1;
            return Ok(());
        }

        let mut index = 0;
        loop {
            let old_path = self.current_path.clone();
            self.current_path = format!("{}/{}", self.current_path, index);
            self.validate_value()?;
            self.current_path = old_path;
            index += 1;
            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                    self.skip_whitespace();
                }
                Some(']') => {
                    self.next_char();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => {
                    return Err(StrictJsonError::ParseError(
                        serde_json::from_str::<()>("expected ',' or ']'").unwrap_err(),
                    ));
                }
            }
        }
    }

    fn validate_string(&mut self) -> Result<String, StrictJsonError> {
        super::decode::parse_json_string_impl(&mut self.chars)
    }

    fn validate_number(&mut self) -> Result<(), StrictJsonError> {
        if self.peek_char() == Some('-') {
            self.next_char();
        }

        match self.peek_char() {
            Some('0') => {
                self.next_char();
            }
            Some(c) if c.is_ascii_digit() => {
                while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    self.next_char();
                }
            }
            _ => {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit").unwrap_err(),
                ));
            }
        }

        if self.peek_char() == Some('.') {
            self.next_char();
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit after decimal point").unwrap_err(),
                ));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }

        if matches!(self.peek_char(), Some('e') | Some('E')) {
            self.next_char();
            if matches!(self.peek_char(), Some('+') | Some('-')) {
                self.next_char();
            }
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError(
                    serde_json::from_str::<()>("expected digit in exponent").unwrap_err(),
                ));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }
        Ok(())
    }

    fn validate_bool(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("true") || self.consume_keyword("false") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("expected true or false").unwrap_err(),
            ))
        }
    }

    fn validate_null(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("null") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("expected null").unwrap_err(),
            ))
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let mut temp_chars = self.chars.clone();
        for expected in keyword.chars() {
            match temp_chars.next() {
                Some((_, c)) if c == expected => {}
                _ => return false,
            }
        }
        self.chars = temp_chars;
        true
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(|c| c.is_whitespace()) {
            self.next_char();
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn next_char(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    fn expect_char(&mut self, expected: char) -> Result<(), StrictJsonError> {
        match self.next_char() {
            Some((_, c)) if c == expected => Ok(()),
            Some((_, c)) => Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>(&format!("expected '{}', got '{}'", expected, c))
                    .unwrap_err(),
            )),
            None => Err(StrictJsonError::ParseError(
                serde_json::from_str::<()>("unexpected end of input").unwrap_err(),
            )),
        }
    }
}
