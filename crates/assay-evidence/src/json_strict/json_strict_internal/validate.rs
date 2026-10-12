use crate::json_strict::dupkeys::ObjectKeyTracker;
use crate::json_strict::{
    check_next, BoundedJsonError, BoundedJsonHooks, BoundedJsonLimits, BudgetDimension, BudgetHit,
    BudgetLimit, PathSegment, StrictJsonError, StringClass,
};
use std::str::CharIndices;

struct BudgetState<'h> {
    limits: BoundedJsonLimits,
    hooks: &'h mut dyn BoundedJsonHooks,
    path: Vec<PathSegment>,
    active_entries: Vec<(usize, BudgetLimit)>,
}

impl BudgetState<'_> {
    fn charge_raw_end(&self, end: usize) -> Result<(), BudgetHit> {
        for (start, limit) in &self.active_entries {
            check_next(
                0,
                end.saturating_sub(*start) as u64,
                *limit,
                BudgetDimension::EntryBytes,
            )?;
        }
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<(), BudgetHit> {
        check_next(0, depth as u64, self.limits.depth, BudgetDimension::Depth).map(|_| ())
    }
}

pub(crate) struct JsonValidator<'a, 'h> {
    chars: std::iter::Peekable<CharIndices<'a>>,
    key_tracker: ObjectKeyTracker,
    current_path: String,
    depth: usize,
    max_depth: usize,
    budget: Option<BudgetState<'h>>,
}

impl<'a> JsonValidator<'a, 'static> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self::with_max_depth(input, super::limits::MAX_NESTING_DEPTH)
    }

    pub(crate) fn with_max_depth(input: &'a str, max_depth: usize) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            key_tracker: ObjectKeyTracker::new(),
            current_path: String::new(),
            depth: 0,
            max_depth,
            budget: None,
        }
    }

    pub(crate) fn validate(&mut self) -> Result<(), StrictJsonError> {
        match self.validate_bounded() {
            Ok(()) => Ok(()),
            Err(BoundedJsonError::Strict(error)) => Err(error),
            Err(BoundedJsonError::Budget(_)) => unreachable!("legacy validator has no budget"),
        }
    }
}

impl<'a, 'h> JsonValidator<'a, 'h> {
    pub(crate) fn with_budget(
        input: &'a str,
        limits: BoundedJsonLimits,
        hooks: &'h mut dyn BoundedJsonHooks,
    ) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            key_tracker: ObjectKeyTracker::new(),
            current_path: String::new(),
            depth: 0,
            max_depth: limits.depth.hard.min(usize::MAX as u64) as usize,
            budget: Some(BudgetState {
                limits,
                hooks,
                path: Vec::new(),
                active_entries: Vec::new(),
            }),
        }
    }

    pub(crate) fn validate_bounded(&mut self) -> Result<(), BoundedJsonError> {
        self.skip_whitespace()?;
        self.validate_value()
    }

    fn validate_value(&mut self) -> Result<(), BoundedJsonError> {
        self.skip_whitespace()?;
        match self.peek_char() {
            Some('{') => self.validate_object(),
            Some('[') => self.validate_array(),
            Some('"') => self.validate_string(false).map(|_| ()),
            Some(c) if c == '-' || c.is_ascii_digit() => self.validate_number(),
            Some('t') | Some('f') => self.validate_bool(),
            Some('n') => self.validate_null(),
            Some(c) => Err(parse_error(&format!("unexpected char '{c}'"))),
            None => Ok(()),
        }
    }

    fn enter_container(&mut self) -> Result<(), BoundedJsonError> {
        self.depth += 1;
        if let Some(state) = &self.budget {
            state.check_depth(self.depth)?;
        } else if self.depth > self.max_depth {
            return Err(StrictJsonError::NestingTooDeep { depth: self.depth }.into());
        }
        Ok(())
    }

    fn validate_object(&mut self) -> Result<(), BoundedJsonError> {
        let start = self.chars.peek().map_or(0, |(pos, _)| *pos);
        let mut owns_entry_span = false;
        if let Some(state) = &mut self.budget {
            if let Some(limit) = state.hooks.object_start(&state.path)? {
                state.active_entries.push((start, limit));
                owns_entry_span = true;
            }
        }
        self.expect_char('{')?;
        self.skip_whitespace()?;
        self.enter_container()?;
        self.key_tracker.enter_object(self.current_path.clone());
        if self.peek_char() == Some('}') {
            self.next_char()?;
            self.key_tracker.exit_object();
            self.depth -= 1;
            if owns_entry_span {
                self.budget
                    .as_mut()
                    .expect("bounded object span has budget")
                    .active_entries
                    .pop();
            }
            return Ok(());
        }
        loop {
            self.skip_whitespace()?;
            let key = self.validate_string(true)?;
            self.key_tracker.push_key(key.clone())?;
            self.skip_whitespace()?;
            self.expect_char(':')?;
            let old_path = self.current_path.clone();
            self.current_path = if old_path.is_empty() {
                format!("/{key}")
            } else {
                format!("{old_path}/{key}")
            };
            if let Some(state) = &mut self.budget {
                state.path.push(PathSegment::Key(key));
            }
            self.validate_value()?;
            if let Some(state) = &mut self.budget {
                state.path.pop();
            }
            self.current_path = old_path;
            self.skip_whitespace()?;
            match self.peek_char() {
                Some(',') => {
                    self.next_char()?;
                }
                Some('}') => {
                    self.next_char()?;
                    self.key_tracker.exit_object();
                    self.depth -= 1;
                    if owns_entry_span {
                        self.budget
                            .as_mut()
                            .expect("bounded object span has budget")
                            .active_entries
                            .pop();
                    }
                    return Ok(());
                }
                _ => return Err(parse_error("expected ',' or '}'")),
            }
        }
    }

    fn validate_array(&mut self) -> Result<(), BoundedJsonError> {
        self.expect_char('[')?;
        self.skip_whitespace()?;
        self.enter_container()?;
        if self.peek_char() == Some(']') {
            self.next_char()?;
            self.depth -= 1;
            return Ok(());
        }
        let mut index = 0usize;
        loop {
            self.skip_whitespace()?;
            let start = self.chars.peek().map_or(0, |(pos, _)| *pos);
            if let Some(state) = &mut self.budget {
                let entry_limit = state.hooks.item_start(&state.path, index)?;
                state.path.push(PathSegment::Index(index));
                if let Some(limit) = entry_limit {
                    state.active_entries.push((start, limit));
                }
            }
            let old_path = self.current_path.clone();
            self.current_path = format!("{}/{}", self.current_path, index);
            self.validate_value()?;
            self.current_path = old_path;
            if let Some(state) = &mut self.budget {
                state.path.pop();
                if state
                    .active_entries
                    .last()
                    .is_some_and(|(entry_start, _)| *entry_start == start)
                {
                    state.active_entries.pop();
                }
            }
            index += 1;
            self.skip_whitespace()?;
            match self.peek_char() {
                Some(',') => {
                    self.next_char()?;
                    self.skip_whitespace()?;
                }
                Some(']') => {
                    self.next_char()?;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(parse_error("expected ',' or ']'")),
            }
        }
    }

    fn validate_string(&mut self, is_key: bool) -> Result<String, BoundedJsonError> {
        if self.budget.is_none() {
            return super::decode::parse_json_string_impl(&mut self.chars)
                .map_err(BoundedJsonError::Strict);
        }
        let decoded_limit = self.budget.as_ref().map(|state| {
            if is_key {
                (state.limits.key_bytes, BudgetDimension::KeyBytes)
            } else {
                match state.hooks.string_class(&state.path) {
                    StringClass::Payload => (
                        state.limits.payload_string_bytes,
                        BudgetDimension::PayloadStringBytes,
                    ),
                    StringClass::NonPayload => (
                        state.limits.non_payload_string_bytes,
                        BudgetDimension::NonPayloadStringBytes,
                    ),
                }
            }
        });
        let (chars, budget) = (&mut self.chars, &self.budget);
        crate::json_strict::scan::parse_json_string_bounded(chars, decoded_limit, &mut |end| {
            match budget {
                Some(state) => state.charge_raw_end(end),
                None => Ok(()),
            }
        })
    }

    fn validate_number(&mut self) -> Result<(), BoundedJsonError> {
        if self.peek_char() == Some('-') {
            self.next_char()?;
        }
        match self.peek_char() {
            Some('0') => {
                self.next_char()?;
            }
            Some(c) if c.is_ascii_digit() => {
                while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    self.next_char()?;
                }
            }
            _ => return Err(parse_error("expected digit")),
        }
        if self.peek_char() == Some('.') {
            self.next_char()?;
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(parse_error("expected digit after decimal point"));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char()?;
            }
        }
        if matches!(self.peek_char(), Some('e') | Some('E')) {
            self.next_char()?;
            if matches!(self.peek_char(), Some('+') | Some('-')) {
                self.next_char()?;
            }
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(parse_error("expected digit in exponent"));
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char()?;
            }
        }
        Ok(())
    }

    fn validate_bool(&mut self) -> Result<(), BoundedJsonError> {
        if self.consume_keyword("true")? || self.consume_keyword("false")? {
            Ok(())
        } else {
            Err(parse_error("expected true or false"))
        }
    }

    fn validate_null(&mut self) -> Result<(), BoundedJsonError> {
        if self.consume_keyword("null")? {
            Ok(())
        } else {
            Err(parse_error("expected null"))
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> Result<bool, BoundedJsonError> {
        let mut temp = self.chars.clone();
        for expected in keyword.chars() {
            match temp.peek().copied() {
                Some((pos, c)) => {
                    let end = pos + c.len_utf8();
                    if let Some(state) = &self.budget {
                        state.charge_raw_end(end)?;
                    }
                    temp.next();
                    if c != expected {
                        return Ok(false);
                    }
                }
                None => return Ok(false),
            }
        }
        self.chars = temp;
        Ok(true)
    }

    fn skip_whitespace(&mut self) -> Result<(), BoundedJsonError> {
        while self.peek_char().is_some_and(|c| c.is_whitespace()) {
            self.next_char()?;
        }
        Ok(())
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn next_char(&mut self) -> Result<Option<(usize, char)>, BoundedJsonError> {
        let next = self.chars.peek().copied();
        if let (Some(state), Some((pos, c))) = (&self.budget, next) {
            state.charge_raw_end(pos + c.len_utf8())?;
        }
        if next.is_some() {
            self.chars.next();
        }
        Ok(next)
    }

    fn expect_char(&mut self, expected: char) -> Result<(), BoundedJsonError> {
        match self.next_char()? {
            Some((_, c)) if c == expected => Ok(()),
            Some((_, c)) => Err(parse_error(&format!("expected '{expected}', got '{c}'"))),
            None => Err(parse_error("unexpected end of input")),
        }
    }
}

fn parse_error(message: &str) -> BoundedJsonError {
    BoundedJsonError::Strict(StrictJsonError::ParseError(
        serde_json::from_str::<()>(message).unwrap_err(),
    ))
}
