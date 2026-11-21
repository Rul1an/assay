//! Scoped access to the tokens of the libyaml backend used by yaml_serde.

use super::{ReadError, ReadResult};
use libyaml_rs as yaml;
use std::{ffi::CStr, mem::MaybeUninit};
use std::{format, string::String};

pub(super) struct Token(yaml::yaml_token_t);

impl Token {
    pub(super) fn kind(&self) -> yaml::yaml_token_type_t {
        self.0.type_
    }

    pub(super) fn position(&self) -> String {
        format!("line {}", self.0.start_mark.line + 1)
    }

    #[allow(unsafe_code)]
    pub(super) fn scalar(&self) -> &str {
        assert_eq!(self.kind(), yaml::YAML_SCALAR_TOKEN);
        // SAFETY: the discriminant selects the scalar union member. The token owns
        // the indicated UTF-8 buffer through this borrow, including empty scalars.
        let bytes = unsafe {
            let scalar = self.0.data.scalar;
            std::slice::from_raw_parts(scalar.value, scalar.length as usize)
        };
        std::str::from_utf8(bytes).expect("libyaml emits UTF-8 scalars")
    }

    #[allow(unsafe_code)]
    pub(super) fn tag(&self) -> String {
        assert_eq!(self.kind(), yaml::YAML_TAG_TOKEN);
        // SAFETY: both NUL-terminated strings belong to this live tag token.
        let (handle, suffix) = unsafe {
            let tag = self.0.data.tag;
            (
                CStr::from_ptr(tag.handle.cast()),
                CStr::from_ptr(tag.suffix.cast()),
            )
        };
        let suffix = suffix.to_string_lossy();
        if handle.to_bytes().is_empty() {
            format!("!<{suffix}>")
        } else {
            format!("{}{suffix}", handle.to_string_lossy())
        }
    }
}

impl Drop for Token {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: scan transfers one initialized token to each guard.
        unsafe { yaml::yaml_token_delete(&mut self.0) };
    }
}

struct Parser<'a>(&'a mut MaybeUninit<yaml::yaml_parser_t>);

impl Drop for Parser<'_> {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: this guard borrows initialized, address-stable parser storage.
        unsafe { yaml::yaml_parser_delete(self.0.as_mut_ptr()) };
    }
}

#[allow(unsafe_code)]
pub(super) fn scan(input: &str, mut visit: impl FnMut(&Token) -> ReadResult<()>) -> ReadResult<()> {
    let mut storage = MaybeUninit::uninit();
    // SAFETY: initialize writes into aligned storage. The guard is installed
    // only on success, and its borrow keeps that storage at the same address.
    if unsafe { yaml::yaml_parser_initialize(storage.as_mut_ptr()) }.fail {
        return Err(ReadError::ParseError {
            message: "could not initialize YAML parser".into(),
        });
    }
    let parser = Parser(&mut storage);
    // SAFETY: input outlives the parser; its UTF-8 bytes are never modified.
    unsafe {
        yaml::yaml_parser_set_encoding(parser.0.as_mut_ptr(), yaml::YAML_UTF8_ENCODING);
        yaml::yaml_parser_set_input_string(
            parser.0.as_mut_ptr(),
            input.as_ptr(),
            input.len() as u64,
        );
    }
    loop {
        let mut raw = MaybeUninit::uninit();
        // SAFETY: both pointers address live storage; success initializes and
        // transfers ownership of the token. Stop at the first error or stream end.
        if unsafe { yaml::yaml_parser_scan(parser.0.as_mut_ptr(), raw.as_mut_ptr()) }.fail {
            // SAFETY: a failed scan leaves the initialized parser's problem mark available.
            let failed = unsafe { parser.0.assume_init_ref() };
            let mark = failed.problem_mark;
            let problem = if failed.problem.is_null() {
                "invalid YAML token".into()
            } else {
                // SAFETY: libyaml retains the NUL-terminated error text until parser deletion.
                unsafe { CStr::from_ptr(failed.problem.cast()) }.to_string_lossy()
            };
            return Err(ReadError::ParseError {
                message: format!(
                    "{problem} at line {} column {}",
                    mark.line + 1,
                    mark.column + 1
                ),
            });
        }
        // SAFETY: the call above initialized raw and we transfer it to one guard.
        let token = Token(unsafe { raw.assume_init() });
        if token.kind() == yaml::YAML_STREAM_END_TOKEN {
            return Ok(());
        }
        visit(&token)?;
    }
}
