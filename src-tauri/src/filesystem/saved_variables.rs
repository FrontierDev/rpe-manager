//! Data-only access to the `RPEngineManagerDB` SavedVariable assignment.
//!
//! This module never evaluates Lua. It recognizes Lua literals and balanced
//! source structure only so the dedicated Manager table can be replaced while
//! preserving every byte outside that assignment.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write as FmtWrite},
    ops::Range,
};

use serde_json::{Map, Number, Value};

use crate::protocol::{
    InstalledPackage, OperationEnvelope, OperationKind, OperationResult, OperationResultStatus,
    PackageType, ProtocolState, ProtocolValidationError, ResultOperation, PROTOCOL_VERSION,
};

const MANAGER_GLOBAL: &str = "RPEngineManagerDB";
const DATASET_GLOBAL: &str = "RPEngineDatasetDB";
const RULESET_GLOBAL: &str = "RPEngineRulesetDB";
const MAX_LITERAL_NESTING: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagerSavedVariables {
    Absent,
    Present(ProtocolState),
}

#[derive(Debug, Eq, PartialEq)]
pub enum SavedVariablesError {
    LuaSyntax { offset: usize, detail: String },
    ManagerSyntax { offset: usize, detail: String },
    DuplicateManagerAssignment,
    MalformedManagerData(String),
    UnsupportedProtocolVersion(String),
    ProtocolValidation(ProtocolValidationError),
}

impl fmt::Display for SavedVariablesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LuaSyntax { offset, detail } => {
                write!(
                    formatter,
                    "SavedVariables syntax error at byte {offset}: {detail}"
                )
            }
            Self::ManagerSyntax { offset, detail } => write!(
                formatter,
                "{MANAGER_GLOBAL} assignment is invalid at byte {offset}: {detail}"
            ),
            Self::DuplicateManagerAssignment => {
                write!(
                    formatter,
                    "SavedVariables contains multiple {MANAGER_GLOBAL} assignments"
                )
            }
            Self::MalformedManagerData(message) => {
                write!(formatter, "{MANAGER_GLOBAL} data is malformed: {message}")
            }
            Self::UnsupportedProtocolVersion(version) => write!(
                formatter,
                "{MANAGER_GLOBAL} uses unsupported protocol version {version}"
            ),
            Self::ProtocolValidation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SavedVariablesError {}

impl From<ProtocolValidationError> for SavedVariablesError {
    fn from(error: ProtocolValidationError) -> Self {
        match error {
            ProtocolValidationError::UnsupportedProtocolVersion(version) => {
                Self::UnsupportedProtocolVersion(version.to_string())
            }
            other => Self::ProtocolValidation(other),
        }
    }
}

/// Reads and validates the Manager assignment without evaluating any Lua.
pub fn parse_manager_state(source: &str) -> Result<ManagerSavedVariables, SavedVariablesError> {
    match locate_manager_assignment(source)? {
        Some(assignment) => Ok(ManagerSavedVariables::Present(assignment.state)),
        None => Ok(ManagerSavedVariables::Absent),
    }
}

/// Checks the literal authored RPE databases without evaluating Lua. This is
/// used only to reconcile a manager manifest when RPE data is already gone.
pub fn native_content_contains_id(
    source: &str,
    native_id: &str,
) -> Result<bool, SavedVariablesError> {
    let mut lexer = Lexer::new(source);
    while let Some(token) = lexer.next_token()? {
        let TokenKind::Identifier(name) = token.kind else {
            continue;
        };
        if name != DATASET_GLOBAL && name != RULESET_GLOBAL {
            continue;
        }
        if !matches!(
            lexer
                .peek_token()
                .map(|token| token.map(|value| &value.kind))?,
            Some(TokenKind::Equals)
        ) {
            continue;
        }
        lexer.next_token()?;
        let (value, _) = DataParser::new(&mut lexer).parse_value(token.end)?;
        if lua_value_contains_id(&value, native_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn lua_value_contains_id(value: &LuaValue, native_id: &str) -> bool {
    match value {
        LuaValue::String(value) => value.as_slice() == native_id.as_bytes(),
        LuaValue::Table(table) => table.fields.iter().any(|(key, value)| {
            matches!(key, LuaKey::String(key) if key == native_id)
                || lua_value_contains_id(value, native_id)
        }),
        LuaValue::Nil | LuaValue::Boolean(_) | LuaValue::Number(_) => false,
    }
}

/// Serializes one deterministic v1 assignment. It does not include a trailing
/// newline so callers can preserve the target statement's surrounding bytes.
pub fn serialize_manager_assignment(state: &ProtocolState) -> Result<String, SavedVariablesError> {
    state.validate()?;
    let mut output = String::new();
    output.push_str(MANAGER_GLOBAL);
    output.push_str(" = {\n");
    writeln!(output, "    protocolVersion = {},", state.protocol_version)
        .expect("writing to String cannot fail");

    output.push_str("    pendingOperations = {");
    if state.pending_operations.is_empty() {
        output.push_str("},\n");
    } else {
        output.push('\n');
        for operation in &state.pending_operations {
            write_operation(&mut output, operation, 2);
            output.push_str(",\n");
        }
        output.push_str("    },\n");
    }

    output.push_str("    installedPackages = {");
    if state.installed_packages.is_empty() {
        output.push_str("},\n");
    } else {
        output.push('\n');
        for (catalogue_id, package) in &state.installed_packages {
            write_indent(&mut output, 2);
            output.push('[');
            write_lua_string(&mut output, catalogue_id);
            output.push_str("] = {\n");
            write_installed_package(&mut output, package, 3);
            output.push_str("        },\n");
        }
        output.push_str("    },\n");
    }

    output.push_str("    operationResults = {");
    if state.operation_results.is_empty() {
        output.push_str("},\n");
    } else {
        output.push('\n');
        for (request_id, result) in &state.operation_results {
            write_indent(&mut output, 2);
            output.push('[');
            write_lua_string(&mut output, request_id);
            output.push_str("] = {\n");
            write_operation_result(&mut output, result, 3);
            output.push_str("        },\n");
        }
        output.push_str("    },\n");
    }

    output.push('}');
    Ok(output)
}

/// Replaces only the Manager assignment, or appends it when absent. Existing
/// malformed/unsupported Manager data is never overwritten as a fallback.
pub fn replace_manager_assignment(
    source: &str,
    state: &ProtocolState,
) -> Result<String, SavedVariablesError> {
    let assignment = serialize_manager_assignment(state)?;
    match locate_manager_assignment(source)? {
        Some(existing) => {
            let mut output = String::with_capacity(
                source.len() - (existing.span.end - existing.span.start) + assignment.len(),
            );
            output.push_str(&source[..existing.span.start]);
            output.push_str(&assignment);
            output.push_str(&source[existing.span.end..]);
            Ok(output)
        }
        None => {
            let mut output = String::with_capacity(source.len() + assignment.len() + 2);
            output.push_str(source);
            if !source.is_empty() && !source.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(&assignment);
            output.push('\n');
            Ok(output)
        }
    }
}

#[derive(Clone, Debug)]
struct LocatedAssignment {
    span: Range<usize>,
    state: ProtocolState,
}

fn locate_manager_assignment(
    source: &str,
) -> Result<Option<LocatedAssignment>, SavedVariablesError> {
    let mut lexer = Lexer::new(source);
    let mut delimiters = Vec::new();
    let mut blocks = Vec::new();
    let mut pending_if = 0_usize;
    let mut located: Option<LocatedAssignment> = None;

    while let Some(token) = lexer.next_token()? {
        if delimiters.is_empty() {
            if let TokenKind::Identifier(identifier) = &token.kind {
                if identifier == MANAGER_GLOBAL {
                    let next = lexer.peek_token()?.cloned();
                    let is_assignment = matches!(
                        next.as_ref().map(|candidate| &candidate.kind),
                        Some(TokenKind::Equals)
                    );
                    if is_assignment {
                        if !blocks.is_empty() || pending_if != 0 {
                            return Err(SavedVariablesError::ManagerSyntax {
                                offset: token.start,
                                detail: "the assignment must be a top-level global statement"
                                    .to_owned(),
                            });
                        }
                        if !is_statement_start(source, token.start) {
                            return Err(SavedVariablesError::ManagerSyntax {
                                offset: token.start,
                                detail:
                                    "unexpected syntax precedes the global assignment on this line"
                                        .to_owned(),
                            });
                        }
                        if located.is_some() {
                            return Err(SavedVariablesError::DuplicateManagerAssignment);
                        }

                        lexer.next_token()?;
                        let (value, value_end) = {
                            let mut parser = DataParser::new(&mut lexer);
                            parser.parse_value(0)?
                        };
                        let state = manager_state_from_value(value)?;
                        validate_statement_end(source, value_end, lexer.peek_token()?)?;
                        located = Some(LocatedAssignment {
                            span: token.start..value_end,
                            state,
                        });
                        continue;
                    }

                    if is_statement_start(source, token.start) {
                        return Err(SavedVariablesError::ManagerSyntax {
                            offset: token.start,
                            detail: "expected '=' after the global name".to_owned(),
                        });
                    }
                }
            }
        } else if let TokenKind::Identifier(identifier) = &token.kind {
            if identifier == MANAGER_GLOBAL
                && matches!(
                    lexer.peek_token()?.map(|candidate| &candidate.kind),
                    Some(TokenKind::Equals)
                )
            {
                return Err(SavedVariablesError::ManagerSyntax {
                    offset: token.start,
                    detail: "the assignment must be a top-level global statement".to_owned(),
                });
            }
        }

        update_structure(&token, &mut delimiters, &mut blocks, &mut pending_if)?;
    }

    if let Some(delimiter) = delimiters.last() {
        return Err(SavedVariablesError::LuaSyntax {
            offset: delimiter.offset,
            detail: "unclosed delimiter".to_owned(),
        });
    }
    if let Some(block) = blocks.last() {
        return Err(SavedVariablesError::LuaSyntax {
            offset: block.offset,
            detail: "unclosed block".to_owned(),
        });
    }
    if pending_if != 0 {
        return Err(SavedVariablesError::LuaSyntax {
            offset: source.len(),
            detail: "an if statement is missing then".to_owned(),
        });
    }

    Ok(located)
}

fn is_statement_start(source: &str, offset: usize) -> bool {
    let line_start = source[..offset]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let prefix = source[line_start..offset].trim_start_matches('\u{feff}');
    let before_semicolon = prefix
        .rsplit_once(';')
        .map(|(_, tail)| tail)
        .unwrap_or(prefix);
    before_semicolon
        .chars()
        .all(|character| character == ' ' || character == '\t' || character == '\r')
}

fn validate_statement_end(
    source: &str,
    value_end: usize,
    next: Option<&Token>,
) -> Result<(), SavedVariablesError> {
    let Some(next) = next else {
        return Ok(());
    };
    if matches!(&next.kind, TokenKind::Semicolon) {
        return Err(SavedVariablesError::ManagerSyntax {
            offset: next.start,
            detail: "a semicolon is not part of the SavedVariables assignment format".to_owned(),
        });
    }
    let boundary = &source[value_end..next.start];
    let new_line = boundary.contains('\n') || boundary.contains('\r');
    let follows_global_assignment = match &next.kind {
        TokenKind::Identifier(_) if new_line => source[next.end..]
            .split(['\n', '\r'])
            .next()
            .is_some_and(|rest| rest.trim_start().starts_with('=')),
        _ => false,
    };
    if follows_global_assignment {
        Ok(())
    } else {
        Err(SavedVariablesError::ManagerSyntax {
            offset: next.start,
            detail: "expected a new top-level assignment after the Manager table".to_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum DelimiterKind {
    Brace,
    Bracket,
    Parenthesis,
}

#[derive(Clone, Copy, Debug)]
struct Delimiter {
    kind: DelimiterKind,
    offset: usize,
}

#[derive(Clone, Copy, Debug)]
enum BlockKind {
    If,
    Do,
    Function,
    Repeat,
}

#[derive(Clone, Copy, Debug)]
struct Block {
    kind: BlockKind,
    offset: usize,
}

fn update_structure(
    token: &Token,
    delimiters: &mut Vec<Delimiter>,
    blocks: &mut Vec<Block>,
    pending_if: &mut usize,
) -> Result<(), SavedVariablesError> {
    let at_top_level = delimiters.is_empty();
    match &token.kind {
        TokenKind::LeftBrace => delimiters.push(Delimiter {
            kind: DelimiterKind::Brace,
            offset: token.start,
        }),
        TokenKind::LeftBracket => delimiters.push(Delimiter {
            kind: DelimiterKind::Bracket,
            offset: token.start,
        }),
        TokenKind::LeftParenthesis => delimiters.push(Delimiter {
            kind: DelimiterKind::Parenthesis,
            offset: token.start,
        }),
        TokenKind::RightBrace => pop_delimiter(delimiters, DelimiterKind::Brace, token.start)?,
        TokenKind::RightBracket => pop_delimiter(delimiters, DelimiterKind::Bracket, token.start)?,
        TokenKind::RightParenthesis => {
            pop_delimiter(delimiters, DelimiterKind::Parenthesis, token.start)?
        }
        TokenKind::Identifier(ref identifier) if at_top_level => match identifier.as_str() {
            "function" => blocks.push(Block {
                kind: BlockKind::Function,
                offset: token.start,
            }),
            "if" => *pending_if += 1,
            "then" if *pending_if > 0 => {
                *pending_if -= 1;
                blocks.push(Block {
                    kind: BlockKind::If,
                    offset: token.start,
                });
            }
            "do" => blocks.push(Block {
                kind: BlockKind::Do,
                offset: token.start,
            }),
            "repeat" => blocks.push(Block {
                kind: BlockKind::Repeat,
                offset: token.start,
            }),
            "end" => match blocks.pop() {
                Some(Block {
                    kind: BlockKind::Repeat,
                    offset,
                }) => {
                    return Err(SavedVariablesError::LuaSyntax {
                        offset,
                        detail: "repeat blocks must terminate with until".to_owned(),
                    })
                }
                Some(_) => {}
                None => {
                    return Err(SavedVariablesError::LuaSyntax {
                        offset: token.start,
                        detail: "end has no matching block".to_owned(),
                    })
                }
            },
            "until" => match blocks.pop() {
                Some(Block {
                    kind: BlockKind::Repeat,
                    ..
                }) => {}
                Some(block) => {
                    return Err(SavedVariablesError::LuaSyntax {
                        offset: block.offset,
                        detail: "until does not close a repeat block".to_owned(),
                    })
                }
                None => {
                    return Err(SavedVariablesError::LuaSyntax {
                        offset: token.start,
                        detail: "until has no matching repeat".to_owned(),
                    })
                }
            },
            _ => {}
        },
        _ => {}
    }

    if delimiters.len() > MAX_LITERAL_NESTING {
        return Err(SavedVariablesError::LuaSyntax {
            offset: token.start,
            detail: "delimiter nesting exceeds the supported limit".to_owned(),
        });
    }
    if blocks.len() > MAX_LITERAL_NESTING || *pending_if > MAX_LITERAL_NESTING {
        return Err(SavedVariablesError::LuaSyntax {
            offset: token.start,
            detail: "block nesting exceeds the supported limit".to_owned(),
        });
    }
    Ok(())
}

fn pop_delimiter(
    delimiters: &mut Vec<Delimiter>,
    expected: DelimiterKind,
    offset: usize,
) -> Result<(), SavedVariablesError> {
    let Some(actual) = delimiters.pop() else {
        return Err(SavedVariablesError::LuaSyntax {
            offset,
            detail: "closing delimiter has no matching opener".to_owned(),
        });
    };
    let matched = matches!(
        (actual.kind, expected),
        (DelimiterKind::Brace, DelimiterKind::Brace)
            | (DelimiterKind::Bracket, DelimiterKind::Bracket)
            | (DelimiterKind::Parenthesis, DelimiterKind::Parenthesis)
    );
    if matched {
        Ok(())
    } else {
        Err(SavedVariablesError::LuaSyntax {
            offset,
            detail: "closing delimiter does not match its opener".to_owned(),
        })
    }
}

#[derive(Clone, Debug)]
enum TokenKind {
    Identifier(String),
    String(Vec<u8>),
    Number(String),
    True,
    False,
    Nil,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParenthesis,
    RightParenthesis,
    Comma,
    Semicolon,
    Equals,
    Other(char),
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

struct Lexer<'source> {
    source: &'source str,
    position: usize,
    lookahead: Option<Token>,
}

impl<'source> Lexer<'source> {
    fn new(source: &'source str) -> Self {
        Self {
            source,
            position: 0,
            lookahead: None,
        }
    }

    fn peek_token(&mut self) -> Result<Option<&Token>, SavedVariablesError> {
        if self.lookahead.is_none() {
            self.lookahead = self.lex_token()?;
        }
        Ok(self.lookahead.as_ref())
    }

    fn next_token(&mut self) -> Result<Option<Token>, SavedVariablesError> {
        if self.lookahead.is_some() {
            return Ok(self.lookahead.take());
        }
        self.lex_token()
    }

    fn lex_token(&mut self) -> Result<Option<Token>, SavedVariablesError> {
        self.skip_trivia()?;
        let bytes = self.source.as_bytes();
        if self.position >= bytes.len() {
            return Ok(None);
        }

        let start = self.position;
        let byte = bytes[self.position];
        let kind = match byte {
            b'{' => {
                self.position += 1;
                TokenKind::LeftBrace
            }
            b'}' => {
                self.position += 1;
                TokenKind::RightBrace
            }
            b'[' => {
                if let Some((equals, content_start)) = long_bracket_open(bytes, self.position) {
                    let (value, end) = self.read_long_string(equals, content_start, start)?;
                    self.position = end;
                    TokenKind::String(value)
                } else {
                    self.position += 1;
                    TokenKind::LeftBracket
                }
            }
            b']' => {
                self.position += 1;
                TokenKind::RightBracket
            }
            b'(' => {
                self.position += 1;
                TokenKind::LeftParenthesis
            }
            b')' => {
                self.position += 1;
                TokenKind::RightParenthesis
            }
            b',' => {
                self.position += 1;
                TokenKind::Comma
            }
            b';' => {
                self.position += 1;
                TokenKind::Semicolon
            }
            b'=' => {
                self.position += 1;
                TokenKind::Equals
            }
            b'\'' | b'"' => TokenKind::String(self.read_short_string(byte, start)?),
            b'0'..=b'9' => TokenKind::Number(self.read_number(start)),
            b'-' if bytes.get(self.position + 1).is_some_and(u8::is_ascii_digit) => {
                TokenKind::Number(self.read_number(start))
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let identifier = self.read_identifier();
                match identifier.as_str() {
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    "nil" => TokenKind::Nil,
                    _ => TokenKind::Identifier(identifier),
                }
            }
            _ => {
                let character = self.source[self.position..]
                    .chars()
                    .next()
                    .expect("position is before end of UTF-8 source");
                self.position += character.len_utf8();
                TokenKind::Other(character)
            }
        };

        Ok(Some(Token {
            kind,
            start,
            end: self.position,
        }))
    }

    fn skip_trivia(&mut self) -> Result<(), SavedVariablesError> {
        let bytes = self.source.as_bytes();
        loop {
            while self.position < bytes.len()
                && matches!(
                    bytes[self.position],
                    b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c
                )
            {
                self.position += 1;
            }

            if self.position == 0 && bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
                self.position = 3;
                continue;
            }

            if bytes.get(self.position..self.position + 2) != Some(b"--") {
                return Ok(());
            }

            let comment_start = self.position;
            self.position += 2;
            if let Some((equals, content_start)) = long_bracket_open(bytes, self.position) {
                let end = find_long_bracket_end(bytes, equals, content_start).ok_or_else(|| {
                    SavedVariablesError::LuaSyntax {
                        offset: comment_start,
                        detail: "unterminated long comment".to_owned(),
                    }
                })?;
                self.position = end;
            } else {
                while self.position < bytes.len() && bytes[self.position] != b'\n' {
                    self.position += 1;
                }
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let bytes = self.source.as_bytes();
        let start = self.position;
        self.position += 1;
        while self.position < bytes.len()
            && matches!(
                bytes[self.position],
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'
            )
        {
            self.position += 1;
        }
        self.source[start..self.position].to_owned()
    }

    fn read_number(&mut self, start: usize) -> String {
        let bytes = self.source.as_bytes();
        if bytes[self.position] == b'-' {
            self.position += 1;
        }
        if bytes.get(self.position) == Some(&b'0')
            && matches!(bytes.get(self.position + 1), Some(b'x' | b'X'))
        {
            self.position += 2;
            while self.position < bytes.len() && bytes[self.position].is_ascii_hexdigit() {
                self.position += 1;
            }
            return self.source[start..self.position].to_owned();
        }

        while self.position < bytes.len() && bytes[self.position].is_ascii_digit() {
            self.position += 1;
        }
        if bytes.get(self.position) == Some(&b'.') && bytes.get(self.position + 1) != Some(&b'.') {
            self.position += 1;
            while self.position < bytes.len() && bytes[self.position].is_ascii_digit() {
                self.position += 1;
            }
        }
        if matches!(bytes.get(self.position), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(bytes.get(self.position), Some(b'+' | b'-')) {
                self.position += 1;
            }
            while self.position < bytes.len() && bytes[self.position].is_ascii_digit() {
                self.position += 1;
            }
        }
        self.source[start..self.position].to_owned()
    }

    fn read_short_string(
        &mut self,
        quote: u8,
        start: usize,
    ) -> Result<Vec<u8>, SavedVariablesError> {
        let bytes = self.source.as_bytes();
        self.position += 1;
        let mut decoded = Vec::new();
        while self.position < bytes.len() {
            let byte = bytes[self.position];
            self.position += 1;
            if byte == quote {
                return Ok(decoded);
            }
            if byte == b'\n' || byte == b'\r' {
                return Err(SavedVariablesError::LuaSyntax {
                    offset: start,
                    detail: "unescaped line break in a short string".to_owned(),
                });
            }
            if byte != b'\\' {
                decoded.push(byte);
                continue;
            }
            if self.position >= bytes.len() {
                break;
            }

            let escape = bytes[self.position];
            self.position += 1;
            match escape {
                b'a' => decoded.push(0x07),
                b'b' => decoded.push(0x08),
                b'f' => decoded.push(0x0c),
                b'n' => decoded.push(b'\n'),
                b'r' => decoded.push(b'\r'),
                b't' => decoded.push(b'\t'),
                b'v' => decoded.push(0x0b),
                b'\\' | b'\'' | b'"' => decoded.push(escape),
                b'\n' => {}
                b'\r' => {
                    if bytes.get(self.position) == Some(&b'\n') {
                        self.position += 1;
                    }
                }
                b'z' => {
                    while self.position < bytes.len()
                        && matches!(
                            bytes[self.position],
                            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c
                        )
                    {
                        self.position += 1;
                    }
                }
                b'x' => {
                    let Some(high) = bytes.get(self.position).and_then(|byte| hex_value(*byte))
                    else {
                        return Err(SavedVariablesError::LuaSyntax {
                            offset: self.position,
                            detail: "\\x escape requires two hexadecimal digits".to_owned(),
                        });
                    };
                    self.position += 1;
                    let Some(low) = bytes.get(self.position).and_then(|byte| hex_value(*byte))
                    else {
                        return Err(SavedVariablesError::LuaSyntax {
                            offset: self.position,
                            detail: "\\x escape requires two hexadecimal digits".to_owned(),
                        });
                    };
                    self.position += 1;
                    decoded.push((high << 4) | low);
                }
                b'0'..=b'9' => {
                    let mut value = u16::from(escape - b'0');
                    for _ in 0..2 {
                        if let Some(next @ b'0'..=b'9') = bytes.get(self.position).copied() {
                            value = value * 10 + u16::from(next - b'0');
                            self.position += 1;
                        } else {
                            break;
                        }
                    }
                    if value > 255 {
                        return Err(SavedVariablesError::LuaSyntax {
                            offset: self.position,
                            detail: "decimal string escape exceeds 255".to_owned(),
                        });
                    }
                    decoded.push(value as u8);
                }
                _ => {
                    return Err(SavedVariablesError::LuaSyntax {
                        offset: self.position - 1,
                        detail: format!("unsupported string escape \\{}", escape as char),
                    })
                }
            }
        }

        Err(SavedVariablesError::LuaSyntax {
            offset: start,
            detail: "unterminated short string".to_owned(),
        })
    }

    fn read_long_string(
        &self,
        equals: usize,
        content_start: usize,
        start: usize,
    ) -> Result<(Vec<u8>, usize), SavedVariablesError> {
        let bytes = self.source.as_bytes();
        let end = find_long_bracket_end(bytes, equals, content_start).ok_or_else(|| {
            SavedVariablesError::LuaSyntax {
                offset: start,
                detail: "unterminated long string".to_owned(),
            }
        })?;
        let mut content_start = content_start;
        if bytes.get(content_start) == Some(&b'\n') {
            content_start += 1;
        } else if bytes.get(content_start..content_start + 2) == Some(b"\r\n") {
            content_start += 2;
        }
        let close_length = equals + 2;
        let content_end = end - close_length;
        let value = bytes[content_start..content_end].to_vec();
        Ok((value, end))
    }
}

fn long_bracket_open(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut index = start + 1;
    while bytes.get(index) == Some(&b'=') {
        index += 1;
    }
    (bytes.get(index) == Some(&b'[')).then_some((index - start - 1, index + 1))
}

fn find_long_bracket_end(bytes: &[u8], equals: usize, mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        if bytes[index] == b']' {
            let close_end = index + equals + 2;
            if close_end <= bytes.len()
                && bytes[index + 1..close_end - 1]
                    .iter()
                    .all(|byte| *byte == b'=')
                && bytes[close_end - 1] == b']'
            {
                return Some(close_end);
            }
        }
        index += 1;
    }
    None
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum LuaValue {
    Nil,
    Boolean(bool),
    Number(String),
    String(Vec<u8>),
    Table(LuaTable),
}

#[derive(Clone, Debug, Default)]
struct LuaTable {
    fields: BTreeMap<LuaKey, LuaValue>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LuaKey {
    String(String),
    Index(usize),
}

struct DataParser<'lexer, 'source> {
    lexer: &'lexer mut Lexer<'source>,
    nesting: usize,
}

impl<'lexer, 'source> DataParser<'lexer, 'source> {
    fn new(lexer: &'lexer mut Lexer<'source>) -> Self {
        Self { lexer, nesting: 0 }
    }

    fn parse_value(&mut self, offset: usize) -> Result<(LuaValue, usize), SavedVariablesError> {
        let token = self
            .lexer
            .next_token()?
            .ok_or_else(|| SavedVariablesError::ManagerSyntax {
                offset,
                detail: "expected a Lua literal value".to_owned(),
            })?;
        let end = token.end;
        let value = match token.kind {
            TokenKind::Nil => LuaValue::Nil,
            TokenKind::True => LuaValue::Boolean(true),
            TokenKind::False => LuaValue::Boolean(false),
            TokenKind::Number(number) => LuaValue::Number(number),
            TokenKind::String(string) => LuaValue::String(string),
            TokenKind::LeftBrace => {
                let (table, end) = self.parse_table(token.start)?;
                return Ok((LuaValue::Table(table), end));
            }
            TokenKind::Other(character) => {
                return Err(SavedVariablesError::ManagerSyntax {
                    offset: token.start,
                    detail: format!("unsupported Lua token {character:?}"),
                })
            }
            other => {
                return Err(SavedVariablesError::ManagerSyntax {
                    offset: token.start,
                    detail: format!(
                        "only nil, booleans, numbers, strings, and literal tables are supported; found {other:?}"
                    ),
                })
            }
        };
        Ok((value, end))
    }

    fn parse_table(&mut self, start: usize) -> Result<(LuaTable, usize), SavedVariablesError> {
        self.nesting += 1;
        if self.nesting > MAX_LITERAL_NESTING {
            return Err(SavedVariablesError::ManagerSyntax {
                offset: start,
                detail: "table nesting exceeds the supported limit".to_owned(),
            });
        }

        let mut table = LuaTable::default();
        let mut next_implicit = 1_usize;
        loop {
            let Some(next) = self.lexer.peek_token()?.cloned() else {
                self.nesting -= 1;
                return Err(SavedVariablesError::ManagerSyntax {
                    offset: start,
                    detail: "unterminated table constructor".to_owned(),
                });
            };
            if matches!(next.kind, TokenKind::RightBrace) {
                let end = self.lexer.next_token()?.expect("peeked token exists").end;
                self.nesting -= 1;
                return Ok((table, end));
            }

            let key = match next.kind {
                TokenKind::LeftBracket => {
                    self.lexer.next_token()?;
                    let key_token = self.lexer.next_token()?.ok_or_else(|| {
                        SavedVariablesError::ManagerSyntax {
                            offset: next.start,
                            detail: "expected a literal table key after '['".to_owned(),
                        }
                    })?;
                    let key = match key_token.kind {
                        TokenKind::String(value) => {
                            let value = String::from_utf8(value).map_err(|_| {
                                SavedVariablesError::ManagerSyntax {
                                    offset: key_token.start,
                                    detail: "table key is not valid UTF-8".to_owned(),
                                }
                            })?;
                            LuaKey::String(value)
                        }
                        TokenKind::Number(value) => {
                            let index = integer_literal(&value)
                                .filter(|value| *value > 0)
                                .and_then(|value| usize::try_from(value).ok())
                                .ok_or_else(|| SavedVariablesError::ManagerSyntax {
                                    offset: key_token.start,
                                    detail: "table indexes must be positive integer literals"
                                        .to_owned(),
                                })?;
                            LuaKey::Index(index)
                        }
                        _ => {
                            return Err(SavedVariablesError::ManagerSyntax {
                                offset: key_token.start,
                                detail: "computed and non-scalar table keys are not supported"
                                    .to_owned(),
                            })
                        }
                    };
                    expect_token(
                        self.lexer,
                        |kind| matches!(kind, TokenKind::RightBracket),
                        "']'",
                    )?;
                    expect_token(self.lexer, |kind| matches!(kind, TokenKind::Equals), "'='")?;
                    key
                }
                TokenKind::Identifier(ref identifier) => {
                    let token = self.lexer.next_token()?.expect("peeked token exists");
                    if matches!(
                        self.lexer.peek_token()?.map(|candidate| &candidate.kind),
                        Some(TokenKind::Equals)
                    ) {
                        self.lexer.next_token()?;
                        LuaKey::String(identifier.clone())
                    } else {
                        return Err(SavedVariablesError::ManagerSyntax {
                            offset: token.start,
                            detail: "bare identifiers are not supported as literal values"
                                .to_owned(),
                        });
                    }
                }
                _ => {
                    let key = LuaKey::Index(next_implicit);
                    next_implicit += 1;
                    key
                }
            };

            let value_offset = self.lexer.peek_token()?.map_or(start, |token| token.start);
            let (value, _) = self.parse_value(value_offset)?;
            insert_table_field(&mut table, key, value, value_offset)?;
            self.consume_field_separator(start)?;
        }
    }

    fn consume_field_separator(&mut self, table_start: usize) -> Result<(), SavedVariablesError> {
        match self.lexer.peek_token()?.map(|token| token.kind.clone()) {
            Some(TokenKind::Comma | TokenKind::Semicolon) => {
                self.lexer.next_token()?;
                Ok(())
            }
            Some(TokenKind::RightBrace) => Ok(()),
            Some(token) => Err(SavedVariablesError::ManagerSyntax {
                offset: self
                    .lexer
                    .peek_token()?
                    .map_or(table_start, |token| token.start),
                detail: format!("expected ',' or '}}' after a table field, found {token:?}"),
            }),
            None => Err(SavedVariablesError::ManagerSyntax {
                offset: table_start,
                detail: "unterminated table constructor".to_owned(),
            }),
        }
    }
}

fn expect_token(
    lexer: &mut Lexer<'_>,
    predicate: impl FnOnce(&TokenKind) -> bool,
    expected: &str,
) -> Result<Token, SavedVariablesError> {
    let token = lexer
        .next_token()?
        .ok_or_else(|| SavedVariablesError::ManagerSyntax {
            offset: lexer.source.len(),
            detail: format!("expected {expected}"),
        })?;
    if predicate(&token.kind) {
        Ok(token)
    } else {
        Err(SavedVariablesError::ManagerSyntax {
            offset: token.start,
            detail: format!("expected {expected}"),
        })
    }
}

fn insert_table_field(
    table: &mut LuaTable,
    key: LuaKey,
    value: LuaValue,
    offset: usize,
) -> Result<(), SavedVariablesError> {
    if table.fields.insert(key.clone(), value).is_some() {
        return Err(SavedVariablesError::ManagerSyntax {
            offset,
            detail: format!("duplicate table key {key:?}"),
        });
    }
    Ok(())
}

fn manager_state_from_value(value: LuaValue) -> Result<ProtocolState, SavedVariablesError> {
    let mut root = table_as_object(value, "root")?;
    let protocol_version = root
        .get("protocolVersion")
        .cloned()
        .ok_or_else(|| malformed("missing protocolVersion"))?;
    match protocol_version {
        LuaValue::Number(raw) => match integer_literal(&raw) {
            Some(version) if version == i128::from(PROTOCOL_VERSION) => {}
            Some(_) => return Err(SavedVariablesError::UnsupportedProtocolVersion(raw)),
            None if raw.parse::<f64>().is_ok() => {
                return Err(malformed(
                    "protocolVersion must be an integer literal equal to 1",
                ))
            }
            None => return Err(SavedVariablesError::UnsupportedProtocolVersion(raw)),
        },
        _ => return Err(malformed("protocolVersion must be an integer")),
    }

    require_exact_keys(
        &root,
        &[
            "protocolVersion",
            "pendingOperations",
            "installedPackages",
            "operationResults",
        ],
        "root",
    )?;
    root.remove("protocolVersion");

    let mut json_root = Map::new();
    json_root.insert(
        "protocolVersion".to_owned(),
        Value::Number(Number::from(PROTOCOL_VERSION)),
    );

    let pending = root
        .remove("pendingOperations")
        .ok_or_else(|| malformed("missing pendingOperations"))?;
    json_root.insert(
        "pendingOperations".to_owned(),
        table_as_array_json(pending, "pendingOperations")?,
    );

    let installed = root
        .remove("installedPackages")
        .ok_or_else(|| malformed("missing installedPackages"))?;
    json_root.insert(
        "installedPackages".to_owned(),
        table_as_object_json(installed, "installedPackages")?,
    );

    let results = root
        .remove("operationResults")
        .ok_or_else(|| malformed("missing operationResults"))?;
    json_root.insert(
        "operationResults".to_owned(),
        table_as_object_json(results, "operationResults")?,
    );

    let state: ProtocolState = serde_json::from_value(Value::Object(json_root))
        .map_err(|error| malformed(format!("protocol-v1 table shape is invalid: {error}")))?;
    state.validate()?;
    Ok(state)
}

fn table_as_object(
    value: LuaValue,
    field: &str,
) -> Result<BTreeMap<String, LuaValue>, SavedVariablesError> {
    let LuaValue::Table(table) = value else {
        return Err(malformed(format!("{field} must be a table")));
    };
    let mut object = BTreeMap::new();
    for (key, value) in table.fields {
        match key {
            LuaKey::String(key) => {
                object.insert(key, value);
            }
            LuaKey::Index(_) => {
                return Err(malformed(format!("{field} must use string keys only")))
            }
        }
    }
    Ok(object)
}

fn require_exact_keys(
    object: &BTreeMap<String, LuaValue>,
    expected: &[&str],
    field: &str,
) -> Result<(), SavedVariablesError> {
    let expected_set = expected.iter().copied().collect::<BTreeSet<_>>();
    for key in object.keys() {
        if !expected_set.contains(key.as_str()) {
            return Err(malformed(format!(
                "{field} contains unexpected field {key:?}"
            )));
        }
    }
    for key in expected {
        if !object.contains_key(*key) {
            return Err(malformed(format!(
                "{field} is missing required field {key}"
            )));
        }
    }
    Ok(())
}

fn table_as_array_json(value: LuaValue, field: &str) -> Result<Value, SavedVariablesError> {
    let LuaValue::Table(table) = value else {
        return Err(malformed(format!("{field} must be an array table")));
    };
    let mut values = Vec::with_capacity(table.fields.len());
    for (expected_index, (key, value)) in table.fields.into_iter().enumerate() {
        match key {
            LuaKey::Index(index) if index == expected_index + 1 => {
                values.push(lua_value_to_json(value, &format!("{field}[{index}]"))?);
            }
            LuaKey::Index(index) => {
                return Err(malformed(format!(
                    "{field} must be dense and 1-based; found index {index} where {} was expected",
                    expected_index + 1
                )))
            }
            LuaKey::String(_) => {
                return Err(malformed(format!("{field} must not contain string keys")))
            }
        }
    }
    Ok(Value::Array(values))
}

fn table_as_object_json(value: LuaValue, field: &str) -> Result<Value, SavedVariablesError> {
    let LuaValue::Table(table) = value else {
        return Err(malformed(format!("{field} must be a map table")));
    };
    let mut object = Map::new();
    for (key, value) in table.fields {
        match key {
            LuaKey::String(key) => {
                object.insert(
                    key.clone(),
                    lua_value_to_json(value, &format!("{field}.{key}"))?,
                );
            }
            LuaKey::Index(_) => {
                return Err(malformed(format!("{field} must use string keys only")))
            }
        }
    }
    Ok(Value::Object(object))
}

fn lua_value_to_json(value: LuaValue, field: &str) -> Result<Value, SavedVariablesError> {
    match value {
        LuaValue::Nil => Err(malformed(format!("{field} must not be nil"))),
        LuaValue::Boolean(value) => Ok(Value::Bool(value)),
        LuaValue::Number(raw) => number_to_json(&raw)
            .map(Value::Number)
            .ok_or_else(|| malformed(format!("{field} has an invalid number literal"))),
        LuaValue::String(value) => String::from_utf8(value)
            .map(Value::String)
            .map_err(|_| malformed(format!("{field} is not valid UTF-8"))),
        LuaValue::Table(table) => {
            if table.fields.is_empty() {
                return Ok(Value::Object(Map::new()));
            }
            let has_strings = table
                .fields
                .keys()
                .any(|key| matches!(key, LuaKey::String(_)));
            let has_indices = table
                .fields
                .keys()
                .any(|key| matches!(key, LuaKey::Index(_)));
            match (has_strings, has_indices) {
                (true, false) => table_as_object_json(LuaValue::Table(table), field),
                (false, true) => table_as_array_json(LuaValue::Table(table), field),
                (true, true) => Err(malformed(format!(
                    "{field} mixes numeric array entries and string map entries"
                ))),
                (false, false) => unreachable!("empty table returned above"),
            }
        }
    }
}

fn number_to_json(raw: &str) -> Option<Number> {
    if let Some(value) = integer_literal(raw) {
        if let Ok(value) = i64::try_from(value) {
            return Some(Number::from(value));
        }
    }
    if let Some(value) = positive_hex_integer(raw) {
        if let Ok(value) = u64::try_from(value) {
            return Some(Number::from(value));
        }
    }
    raw.parse::<f64>().ok().and_then(Number::from_f64)
}

fn integer_literal(raw: &str) -> Option<i128> {
    if raw.contains('.') || raw.contains('e') || raw.contains('E') {
        return None;
    }
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        i128::from_str_radix(hex, 16).ok()
    } else if let Some(hex) = raw.strip_prefix("-0x").or_else(|| raw.strip_prefix("-0X")) {
        i128::from_str_radix(hex, 16).ok().map(|value| -value)
    } else {
        raw.parse::<i128>().ok()
    }
}

fn positive_hex_integer(raw: &str) -> Option<u128> {
    let hex = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X"))?;
    u128::from_str_radix(hex, 16).ok()
}

fn malformed(message: impl Into<String>) -> SavedVariablesError {
    SavedVariablesError::MalformedManagerData(message.into())
}

fn write_operation(output: &mut String, operation: &OperationEnvelope, indent: usize) {
    write_indent(output, indent);
    output.push_str("{\n");
    write_string_field(output, "requestId", &operation.request_id, indent + 1);
    write_string_field(
        output,
        "operation",
        operation_kind_name(operation.operation),
        indent + 1,
    );
    write_string_field(output, "catalogueId", &operation.catalogue_id, indent + 1);
    write_string_field(output, "datasetId", &operation.dataset_id, indent + 1);
    write_number_field(output, "revision", operation.revision, indent + 1);
    write_string_field(output, "hash", &operation.hash, indent + 1);
    if let Some(payload) = &operation.payload {
        write_string_field(output, "payload", payload, indent + 1);
    }
    write_indent(output, indent);
    output.push('}');
}

fn write_installed_package(output: &mut String, package: &InstalledPackage, indent: usize) {
    write_string_field(
        output,
        "packageType",
        match package.package_type {
            PackageType::Dataset => "dataset",
        },
        indent,
    );
    write_string_field(output, "datasetId", &package.dataset_id, indent);
    write_number_field(output, "revision", package.revision, indent);
    write_string_field(output, "hash", &package.hash, indent);
    write_number_field(output, "installedAt", package.installed_at, indent);
}

fn write_operation_result(output: &mut String, result: &OperationResult, indent: usize) {
    write_string_field(output, "requestId", &result.request_id, indent);
    write_string_field(
        output,
        "operation",
        result_operation_name(result.operation),
        indent,
    );
    write_string_field(
        output,
        "status",
        match result.status {
            OperationResultStatus::Succeeded => "succeeded",
            OperationResultStatus::Failed => "failed",
        },
        indent,
    );
    if let Some(catalogue_id) = &result.catalogue_id {
        write_string_field(output, "catalogueId", catalogue_id, indent);
    }
    if let Some(dataset_id) = &result.dataset_id {
        write_string_field(output, "datasetId", dataset_id, indent);
    }
    if let Some(revision) = result.revision {
        write_number_field(output, "revision", revision, indent);
    }
    if let Some(hash) = &result.hash {
        write_string_field(output, "hash", hash, indent);
    }
    if let Some(error) = &result.error {
        write_indent(output, indent);
        output.push_str("error = {\n");
        write_string_field(output, "code", error_code_name(error.code), indent + 1);
        write_string_field(output, "detail", &error.detail, indent + 1);
        write_indent(output, indent);
        output.push_str("},\n");
    }
}

fn write_string_field(output: &mut String, name: &str, value: &str, indent: usize) {
    write_indent(output, indent);
    let _ = write!(output, "{name} = ");
    write_lua_string(output, value);
    output.push_str(",\n");
}

fn write_number_field(output: &mut String, name: &str, value: impl fmt::Display, indent: usize) {
    write_indent(output, indent);
    let _ = writeln!(output, "{name} = {value},");
}

fn write_indent(output: &mut String, indent: usize) {
    for _ in 0..indent * 4 {
        output.push(' ');
    }
}

fn write_lua_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            '\u{0007}' => output.push_str("\\a"),
            '\u{000b}' => output.push_str("\\v"),
            '\u{0000}'..='\u{001f}' | '\u{007f}' => {
                let _ = write!(output, "\\{:03}", character as u32);
            }
            _ => output.push(character),
        }
    }
    output.push('"');
}

fn operation_kind_name(operation: OperationKind) -> &'static str {
    match operation {
        OperationKind::InstallDataset => "install_dataset",
        OperationKind::RemoveDataset => "remove_dataset",
        OperationKind::InstallRuleset => "install_ruleset",
    }
}

fn result_operation_name(operation: ResultOperation) -> &'static str {
    match operation {
        ResultOperation::InstallDataset => "install_dataset",
        ResultOperation::RemoveDataset => "remove_dataset",
        ResultOperation::InstallRuleset => "install_ruleset",
        ResultOperation::Unknown => "unknown",
    }
}

fn error_code_name(code: crate::protocol::ProtocolErrorCode) -> &'static str {
    use crate::protocol::ProtocolErrorCode as Code;
    match code {
        Code::InvalidField => "invalid_field",
        Code::UnsupportedOperation => "unsupported_operation",
        Code::InvalidPayloadFormat => "invalid_payload_format",
        Code::DatasetIdMismatch => "dataset_id_mismatch",
        Code::CatalogueDatasetIdConflict => "catalogue_dataset_id_conflict",
        Code::StaleRevision => "stale_revision",
        Code::RevisionHashConflict => "revision_hash_conflict",
        Code::PackageNotInstalled => "package_not_installed",
        Code::InstalledPackageMismatch => "installed_package_mismatch",
        Code::ImportRejected => "import_rejected",
        Code::RemoveRejected => "remove_rejected",
        Code::InternalError => "internal_error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_MANAGER: &str = include_str!("../../tests/fixtures/saved_variables_no_manager.lua");
    const VALID_MANAGER: &str = include_str!("../../tests/fixtures/saved_variables_manager_v1.lua");
    const MALFORMED_MANAGER: &str =
        include_str!("../../tests/fixtures/saved_variables_malformed_manager.lua");
    const UNSUPPORTED_VERSION: &str =
        include_str!("../../tests/fixtures/saved_variables_unsupported_version.lua");

    #[test]
    fn distinguishes_an_absent_manager_root_from_malformed_data() {
        assert_eq!(
            parse_manager_state(NO_MANAGER).expect("parse unrelated SavedVariables"),
            ManagerSavedVariables::Absent
        );
        assert!(matches!(
            parse_manager_state(MALFORMED_MANAGER),
            Err(SavedVariablesError::ManagerSyntax { .. })
        ));
    }

    #[test]
    fn parses_v1_state_with_nested_tables_and_escaped_strings() {
        let ManagerSavedVariables::Present(state) =
            parse_manager_state(VALID_MANAGER).expect("parse valid fixture")
        else {
            panic!("valid fixture must contain the Manager root");
        };

        assert_eq!(state.protocol_version, PROTOCOL_VERSION);
        assert_eq!(state.pending_operations.len(), 1);
        assert_eq!(
            state.pending_operations[0].operation,
            OperationKind::InstallDataset
        );
        assert_eq!(state.pending_operations[0].dataset_id, "f82db71a");
        assert_eq!(state.installed_packages["esarus-core"].revision, 13);
        assert_eq!(state.operation_results.len(), 1);
        assert_eq!(
            state
                .operation_results
                .values()
                .next()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .detail,
            "Importer rejected text containing {braces}, quotes \\\" and escapes \\\\\\\\."
        );
    }

    #[test]
    fn reports_unsupported_protocol_versions_explicitly() {
        assert!(matches!(
            parse_manager_state(UNSUPPORTED_VERSION),
            Err(SavedVariablesError::UnsupportedProtocolVersion(version)) if version == "2"
        ));
    }

    #[test]
    fn serializes_deterministically_and_round_trips_through_typed_models() {
        let ManagerSavedVariables::Present(state) =
            parse_manager_state(VALID_MANAGER).expect("parse valid fixture")
        else {
            panic!("valid fixture must contain the Manager root");
        };

        let first = serialize_manager_assignment(&state).expect("serialize v1 state");
        let second = serialize_manager_assignment(&state).expect("serialize v1 state again");
        assert_eq!(first, second);

        let ManagerSavedVariables::Present(round_trip) =
            parse_manager_state(&first).expect("parse serialized v1 state")
        else {
            panic!("serialized state must contain the Manager root");
        };
        assert_eq!(round_trip, state);
    }

    #[test]
    fn replaces_only_manager_assignment_and_preserves_unrelated_bytes() {
        let previous = locate_manager_assignment(VALID_MANAGER)
            .expect("locate existing root")
            .expect("fixture root");
        let updated_state = ProtocolState::default();
        let updated = replace_manager_assignment(VALID_MANAGER, &updated_state)
            .expect("replace Manager root");
        let new_assignment = serialize_manager_assignment(&updated_state).unwrap();

        assert_eq!(
            &updated[..previous.span.start],
            &VALID_MANAGER[..previous.span.start]
        );
        assert_eq!(
            &updated[previous.span.start + new_assignment.len()..],
            &VALID_MANAGER[previous.span.end..]
        );
        assert!(updated.contains("RPEngineProfilesDB = {"));
        assert!(updated.contains("RPEngineDatasetDB = {"));
        assert!(updated.contains("RPEngineInventoryDB = {"));
        assert_eq!(
            parse_manager_state(&updated).expect("parse replaced file"),
            ManagerSavedVariables::Present(updated_state)
        );
    }

    #[test]
    fn appends_when_absent_without_rewriting_unrelated_assignments() {
        let state = ProtocolState::default();
        let updated =
            replace_manager_assignment(NO_MANAGER, &state).expect("append Manager assignment");

        assert!(updated.starts_with(NO_MANAGER));
        assert!(updated[NO_MANAGER.len()..].starts_with(MANAGER_GLOBAL));
        assert_eq!(
            parse_manager_state(&updated).expect("parse appended file"),
            ManagerSavedVariables::Present(state)
        );
    }

    #[test]
    fn rejects_duplicate_roots_nested_assignments_and_trailing_code() {
        let duplicate = format!(
            "{VALID_MANAGER}\n{}\n",
            serialize_manager_assignment(&ProtocolState::default()).unwrap()
        );
        assert_eq!(
            parse_manager_state(&duplicate),
            Err(SavedVariablesError::DuplicateManagerAssignment)
        );

        let nested = format!(
            "do\n{}\nend\n",
            serialize_manager_assignment(&ProtocolState::default()).unwrap()
        );
        assert!(matches!(
            parse_manager_state(&nested),
            Err(SavedVariablesError::ManagerSyntax { .. })
        ));

        let trailing = format!(
            "{} + run_code()",
            serialize_manager_assignment(&ProtocolState::default()).unwrap()
        );
        assert!(matches!(
            parse_manager_state(&trailing),
            Err(SavedVariablesError::ManagerSyntax { .. })
        ));

        let trailing_statement = format!(
            "{}\nrun_code()",
            serialize_manager_assignment(&ProtocolState::default()).unwrap()
        );
        assert!(matches!(
            parse_manager_state(&trailing_statement),
            Err(SavedVariablesError::ManagerSyntax { .. })
        ));
    }

    #[test]
    fn serializer_rejects_invalid_typed_protocol_state() {
        let state = ProtocolState {
            protocol_version: 2,
            ..ProtocolState::default()
        };
        assert!(matches!(
            serialize_manager_assignment(&state),
            Err(SavedVariablesError::UnsupportedProtocolVersion(version)) if version == "2"
        ));
    }
}
