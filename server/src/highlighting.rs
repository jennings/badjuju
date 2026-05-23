//! LSP semantic tokens for the generated `*.jujutsu` buffers.
//!
//! Pure line-by-line scanner that emits LSP wire-format tokens (delta-line,
//! delta-start, length, type-index, modifier-bits). Mirrors the categories
//! covered by the tree-sitter highlight queries in
//! `clients/neovim/queries/jujutsu/highlights.scm` so the two highlighting
//! sources can coexist briefly.
//!
//! Positions are reported in Unicode code-point units. All non-ASCII glyphs
//! that appear in jj output (graph chars like `│`, `◆`, `○`, `…`) are on the
//! Basic Multilingual Plane, so the code-point count equals the UTF-16 count
//! the LSP default position encoding expects.
//!
//! The legend in [`TOKEN_TYPES`] and [`TOKEN_MODIFIERS`] is the source of
//! truth for the indices emitted by [`semantic_tokens`]; clients receive the
//! legend through the server's `initialize` response.

use tower_lsp::lsp_types::{SemanticToken, SemanticTokenModifier, SemanticTokenType};

use crate::cursor::BufferKind;

/// Token-type legend. Indexes into this slice are what [`semantic_tokens`]
/// writes into each token's `token_type` field.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,     // 0
    SemanticTokenType::KEYWORD,     // 1
    SemanticTokenType::STRING,      // 2
    SemanticTokenType::TYPE,        // 3
    SemanticTokenType::ENUM_MEMBER, // 4
    SemanticTokenType::NUMBER,      // 5
    SemanticTokenType::OPERATOR,    // 6
];

/// Token-modifier legend. Bit `i` of `token_modifiers_bitset` corresponds to
/// the modifier at index `i`.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[SemanticTokenModifier::DOCUMENTATION];

const TT_COMMENT: u32 = 0;
const TT_KEYWORD: u32 = 1;
const TT_STRING: u32 = 2;
const TT_TYPE: u32 = 3;
const TT_ENUM_MEMBER: u32 = 4;
const TT_NUMBER: u32 = 5;
const TT_OPERATOR: u32 = 6;

const MOD_DOCUMENTATION: u32 = 1 << 0;

/// Graph drawing + working-copy marker characters emitted by jj log. Mirrors
/// the union of `COMMIT_HEADER_CHARS` / `STAT_GRAPH_CHARS` in `cursor.rs` and
/// the `graph_char` token in the tree-sitter grammar.
const GRAPH_CHARS: &[char] = &[
    '@', '*', '│', '◆', '○', '●', '◉', '~', '…', '╭', '╮', '╯', '╰', '├', '─', '┤', '┬', '┴', '┼',
];

/// Compute LSP semantic tokens for the given buffer text.
///
/// `kind` is accepted for forward-compat but does not currently influence the
/// scanner — every `*.jujutsu` buffer follows the same line shape, so one
/// scanner serves all three.
pub fn semantic_tokens(content: &str, _kind: BufferKind) -> Vec<SemanticToken> {
    let mut out = Vec::new();
    let mut prev_line: u32 = 0;
    let mut prev_col: u32 = 0;
    for (line_idx, line) in content.lines().enumerate() {
        let line_idx = line_idx as u32;
        let mut line_tokens: Vec<(u32, u32, u32, u32)> = Vec::new();
        scan_line(line, &mut line_tokens);
        for (col, len, ty, modifier) in line_tokens {
            let delta_line = line_idx - prev_line;
            let delta_start = if delta_line == 0 { col - prev_col } else { col };
            out.push(SemanticToken {
                delta_line,
                delta_start,
                length: len,
                token_type: ty,
                token_modifiers_bitset: modifier,
            });
            prev_line = line_idx;
            prev_col = col;
        }
    }
    out
}

/// Emit `(start_char, length_chars, token_type, modifier_bitset)` tuples for
/// every recognized atom on `line`. Positions are in Unicode code-point units
/// relative to the start of the line.
fn scan_line(line: &str, out: &mut Vec<(u32, u32, u32, u32)>) {
    // Block-level: full-line `JJ:` comment. Shadows any other interpretation
    // so labels embedded after `JJ:` (e.g. `JJ: Mutable: ...`) stay comment-
    // colored rather than splitting into keyword/string.
    if line.starts_with("JJ:") {
        let len = line.chars().count() as u32;
        if len > 0 {
            out.push((0, len, TT_COMMENT, 0));
        }
        return;
    }
    // Block-level: section header `[A-Z][A-Z ]*:` + trailing text.
    if let Some(SectionHeader {
        keyword_chars,
        trailing_chars,
    }) = match_section_header(line)
    {
        out.push((0, keyword_chars, TT_KEYWORD, 0));
        if trailing_chars > 0 {
            out.push((keyword_chars, trailing_chars, TT_STRING, 0));
        }
        return;
    }
    // Optional file_status prefix `[AMDRC][ \t]` followed by inline content.
    let (byte_start, char_start) = if match_file_status_prefix(line).is_some() {
        out.push((0, 2, TT_TYPE, 0));
        (2, 2u32)
    } else {
        (0, 0u32)
    };
    scan_inline(line, byte_start, char_start, out);
}

/// Walk `line[byte_idx..]` emitting inline atoms (graph chars, ids, bookmarks,
/// empty markers). Plain text characters are skipped one at a time.
fn scan_inline(
    line: &str,
    mut byte_idx: usize,
    mut char_idx: u32,
    out: &mut Vec<(u32, u32, u32, u32)>,
) {
    // True when the previous emitted-or-skipped character could be part of a
    // longer identifier (ASCII alphanumeric or `_`). Mirrors the word-boundary
    // assertion that tree-sitter approximates with `_alnum_run` — an
    // identifier atom must not start mid-run.
    let mut prev_alnum = false;
    while byte_idx < line.len() {
        let rest = &line[byte_idx..];
        let Some(c) = rest.chars().next() else {
            break;
        };

        if c == '('
            && let Some(matched_bytes) = match_empty_marker(rest)
        {
            let len_chars = matched_bytes as u32; // ASCII only
            out.push((char_idx, len_chars, TT_COMMENT, MOD_DOCUMENTATION));
            byte_idx += matched_bytes;
            char_idx += len_chars;
            prev_alnum = false;
            continue;
        }
        if c == '['
            && let Some(matched_bytes) = match_bookmark(rest)
        {
            let len_chars = line[byte_idx..byte_idx + matched_bytes].chars().count() as u32;
            out.push((char_idx, len_chars, TT_ENUM_MEMBER, 0));
            byte_idx += matched_bytes;
            char_idx += len_chars;
            prev_alnum = false;
            continue;
        }
        if !prev_alnum
            && c.is_ascii_digit()
            && let Some(matched_bytes) = match_commit_id(rest)
        {
            let len = matched_bytes as u32; // hex digits are ASCII
            out.push((char_idx, len, TT_NUMBER, 0));
            byte_idx += matched_bytes;
            char_idx += len;
            prev_alnum = true;
            continue;
        }
        if !prev_alnum
            && c.is_ascii_lowercase()
            && let Some(matched_bytes) = match_change_id(rest)
        {
            let len = matched_bytes as u32; // lowercase letters are ASCII
            out.push((char_idx, len, TT_NUMBER, 0));
            byte_idx += matched_bytes;
            char_idx += len;
            prev_alnum = true;
            continue;
        }
        if GRAPH_CHARS.contains(&c) {
            out.push((char_idx, 1, TT_OPERATOR, 0));
            byte_idx += c.len_utf8();
            char_idx += 1;
            prev_alnum = false;
            continue;
        }
        // Plain text — advance one character.
        let cb = c.len_utf8();
        byte_idx += cb;
        char_idx += 1;
        prev_alnum = c.is_ascii_alphanumeric() || c == '_';
    }
}

struct SectionHeader {
    keyword_chars: u32,
    trailing_chars: u32,
}

/// Match `^[A-Z][A-Z ]*:`. Returns the keyword length (including the trailing
/// colon) and the trailing-segment length, both in code-point units.
fn match_section_header(line: &str) -> Option<SectionHeader> {
    let mut chars = line.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    let mut keyword_chars: u32 = 1;
    let mut keyword_byte_end = first.len_utf8();
    let mut found_colon = false;
    for (idx, c) in chars {
        if c == ':' {
            keyword_chars += 1;
            keyword_byte_end = idx + c.len_utf8();
            found_colon = true;
            break;
        }
        if c.is_ascii_uppercase() || c == ' ' {
            keyword_chars += 1;
            continue;
        }
        return None;
    }
    if !found_colon {
        return None;
    }
    let trailing_chars = line[keyword_byte_end..].chars().count() as u32;
    Some(SectionHeader {
        keyword_chars,
        trailing_chars,
    })
}

/// Match a file-status prefix: an ASCII letter in `{M, A, D, C, R}` followed
/// by a single space or tab. Returns the byte length consumed (always 2 since
/// both characters are ASCII).
fn match_file_status_prefix(line: &str) -> Option<usize> {
    let mut chars = line.chars();
    let flag = chars.next()?;
    if !matches!(flag, 'M' | 'A' | 'D' | 'C' | 'R') {
        return None;
    }
    let second = chars.next()?;
    if second != ' ' && second != '\t' {
        return None;
    }
    Some(2)
}

/// Match `(empty)` or `(no description set)` at the start of `s`. Returns the
/// matched byte length.
fn match_empty_marker(s: &str) -> Option<usize> {
    if let Some(rest) = s.strip_prefix("(empty)") {
        return Some(s.len() - rest.len());
    }
    if let Some(rest) = s.strip_prefix("(no description set)") {
        return Some(s.len() - rest.len());
    }
    None
}

/// Match `\[[^\]\n]+\]` at the start of `s`. Returns the matched byte length.
fn match_bookmark(s: &str) -> Option<usize> {
    if !s.starts_with('[') {
        return None;
    }
    let mut byte_idx = 1;
    for (content_chars, c) in s[1..].chars().enumerate() {
        let cb = c.len_utf8();
        if c == '\n' {
            return None;
        }
        if c == ']' {
            if content_chars == 0 {
                return None;
            }
            return Some(byte_idx + cb);
        }
        byte_idx += cb;
    }
    None
}

/// Match `[0-9][0-9a-f]{7,39}` followed by a word boundary at the start of
/// `s`. Returns the matched byte length (equals char length since all chars
/// are ASCII).
fn match_commit_id(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    if !(first as char).is_ascii_digit() {
        return None;
    }
    let mut len = 1;
    for &b in &bytes[1..] {
        let c = b as char;
        if c.is_ascii_digit() || ('a'..='f').contains(&c) {
            len += 1;
            if len == 40 {
                break;
            }
        } else {
            break;
        }
    }
    if len < 8 {
        return None;
    }
    if let Some(next) = s.get(len..).and_then(|t| t.chars().next())
        && (next.is_ascii_alphanumeric() || next == '_')
    {
        return None;
    }
    Some(len)
}

/// Match exactly 8 ASCII lowercase letters followed by a word boundary at the
/// start of `s`. Returns the matched byte length (always 8).
fn match_change_id(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.len() < 8 {
        return None;
    }
    for &b in &bytes[..8] {
        if !(b as char).is_ascii_lowercase() {
            return None;
        }
    }
    if let Some(next) = s.get(8..).and_then(|t| t.chars().next())
        && (next.is_ascii_alphanumeric() || next == '_')
    {
        return None;
    }
    Some(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode delta-encoded LSP tokens to absolute `(line, col, len, type, mod)`
    /// tuples for ergonomic assertions.
    fn decode(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut line = 0u32;
        let mut col = 0u32;
        let mut out = Vec::new();
        for t in tokens {
            if t.delta_line != 0 {
                line += t.delta_line;
                col = t.delta_start;
            } else {
                col += t.delta_start;
            }
            out.push((line, col, t.length, t.token_type, t.token_modifiers_bitset));
        }
        out
    }

    mod status_buffer {
        use super::*;

        #[test]
        fn empty_input_emits_nothing() {
            assert_eq!(semantic_tokens("", BufferKind::Status), vec![]);
        }

        #[test]
        fn status_header_no_trailing() {
            let toks = decode(&semantic_tokens("STATUS:\n", BufferKind::Status));
            assert_eq!(toks, vec![(0, 0, 7, TT_KEYWORD, 0)]);
        }

        #[test]
        fn section_header_with_trailing() {
            let content = "STACK: ancestors(reachable(@, mutable()), 2)\n";
            let toks = decode(&semantic_tokens(content, BufferKind::Status));
            let trailing_len = " ancestors(reachable(@, mutable()), 2)".chars().count() as u32;
            assert_eq!(
                toks,
                vec![(0, 0, 6, TT_KEYWORD, 0), (0, 6, trailing_len, TT_STRING, 0),]
            );
        }

        #[test]
        fn file_status_line_emits_only_type_prefix() {
            let toks = decode(&semantic_tokens("M src/main.rs\n", BufferKind::Status));
            assert_eq!(toks, vec![(0, 0, 2, TT_TYPE, 0)]);
        }

        #[test]
        fn file_status_accepts_each_flag() {
            for flag in ['M', 'A', 'D', 'C', 'R'] {
                let line = format!("{flag} path/to/x\n");
                let toks = decode(&semantic_tokens(&line, BufferKind::Status));
                assert_eq!(toks, vec![(0, 0, 2, TT_TYPE, 0)], "flag {flag}");
            }
        }

        #[test]
        fn commit_header_emits_graph_change_and_commit() {
            let toks = decode(&semantic_tokens(
                "@  qpvuntsm 1234abcd\n",
                BufferKind::Status,
            ));
            assert_eq!(
                toks,
                vec![
                    (0, 0, 1, TT_OPERATOR, 0),
                    (0, 3, 8, TT_NUMBER, 0),
                    (0, 12, 8, TT_NUMBER, 0),
                ]
            );
        }

        #[test]
        fn description_line_has_only_graph_token() {
            // Multibyte graph char `│` is one code-point unit.
            let toks = decode(&semantic_tokens(
                "│  description here\n",
                BufferKind::Status,
            ));
            assert_eq!(toks, vec![(0, 0, 1, TT_OPERATOR, 0)]);
        }

        #[test]
        fn empty_marker_gets_documentation_modifier() {
            let toks = decode(&semantic_tokens("(empty) added\n", BufferKind::Status));
            assert_eq!(toks, vec![(0, 0, 7, TT_COMMENT, MOD_DOCUMENTATION)]);
        }

        #[test]
        fn no_description_set_marker() {
            let toks = decode(&semantic_tokens(
                "(no description set)\n",
                BufferKind::Status,
            ));
            assert_eq!(toks, vec![(0, 0, 20, TT_COMMENT, MOD_DOCUMENTATION)]);
        }

        #[test]
        fn bookmark_emits_enum_member() {
            let toks = decode(&semantic_tokens("│ [main] foo\n", BufferKind::Status));
            assert_eq!(
                toks,
                vec![(0, 0, 1, TT_OPERATOR, 0), (0, 2, 6, TT_ENUM_MEMBER, 0),]
            );
        }

        #[test]
        fn change_id_requires_word_boundary() {
            // `qpvuntsma` is 9 lowercase chars — change_id must not eat the
            // first 8 mid-word.
            let toks = decode(&semantic_tokens("@  qpvuntsma\n", BufferKind::Status));
            assert_eq!(toks, vec![(0, 0, 1, TT_OPERATOR, 0)]);
        }

        #[test]
        fn multi_line_uses_delta_encoding() {
            let content = "STATUS:\n\nM src/main.rs\n";
            let toks = decode(&semantic_tokens(content, BufferKind::Status));
            assert_eq!(toks, vec![(0, 0, 7, TT_KEYWORD, 0), (2, 0, 2, TT_TYPE, 0),]);
        }
    }

    mod log_buffer {
        use super::*;

        #[test]
        fn jj_line_is_comment_not_section_header() {
            // `JJ: Mutable: ...` could match the section_header regex except
            // for the leading `JJ:`, which forces comment treatment.
            let line = "JJ: Mutable: ancestors(reachable(@, mutable()))\n";
            let toks = decode(&semantic_tokens(line, BufferKind::Log));
            let len = line.trim_end_matches('\n').chars().count() as u32;
            assert_eq!(toks, vec![(0, 0, len, TT_COMMENT, 0)]);
        }

        #[test]
        fn revset_header_with_at() {
            let toks = decode(&semantic_tokens("REVSET: @\n", BufferKind::Log));
            assert_eq!(
                toks,
                vec![(0, 0, 7, TT_KEYWORD, 0), (0, 7, 2, TT_STRING, 0),]
            );
        }

        #[test]
        fn commit_line_with_bookmark_and_empty_marker() {
            let line = "○  abcdwxyz [main] (empty) wip\n";
            let toks = decode(&semantic_tokens(line, BufferKind::Log));
            // ○ at col 0 (1 char wide), change_id at col 3, bookmark at col 12,
            // empty_marker at col 19.
            assert_eq!(
                toks,
                vec![
                    (0, 0, 1, TT_OPERATOR, 0),
                    (0, 3, 8, TT_NUMBER, 0),
                    (0, 12, 6, TT_ENUM_MEMBER, 0),
                    (0, 19, 7, TT_COMMENT, MOD_DOCUMENTATION),
                ]
            );
        }
    }

    mod diff_buffer {
        use super::*;

        #[test]
        fn revision_header_emits_keyword_and_string() {
            let toks = decode(&semantic_tokens("REVISION: abc123\n", BufferKind::Diff));
            assert_eq!(
                toks,
                vec![(0, 0, 9, TT_KEYWORD, 0), (0, 9, 7, TT_STRING, 0),]
            );
        }

        #[test]
        fn plain_text_emits_no_tokens() {
            let toks = decode(&semantic_tokens("hello world\n", BufferKind::Diff));
            assert_eq!(toks, vec![]);
        }
    }

    /// Sanity-check the internal helpers in isolation.
    mod helpers {
        use super::*;

        #[test]
        fn match_section_header_rejects_lowercase_start() {
            assert!(match_section_header("hello:").is_none());
        }

        #[test]
        fn match_section_header_rejects_no_colon() {
            assert!(match_section_header("ABC DEF").is_none());
        }

        #[test]
        fn match_section_header_handles_internal_space() {
            let h = match_section_header("COMMAND REFERENCE:\n").unwrap();
            assert_eq!(h.keyword_chars, 18);
            // `\n` is stripped by lines() in real usage, but here we keep it
            // — the trailing field includes whatever comes after the colon.
            assert_eq!(h.trailing_chars, 1);
        }

        #[test]
        fn match_commit_id_requires_digit_lead() {
            assert!(match_commit_id("abcdef01").is_none());
            assert_eq!(match_commit_id("0abcdef1").unwrap(), 8);
            assert_eq!(match_commit_id("1234abcd ").unwrap(), 8);
        }

        #[test]
        fn match_commit_id_rejects_short() {
            assert!(match_commit_id("1234567").is_none());
        }

        #[test]
        fn match_change_id_requires_word_boundary() {
            assert_eq!(match_change_id("qpvuntsm ").unwrap(), 8);
            assert!(match_change_id("qpvuntsma").is_none());
            assert!(match_change_id("qpvuntsm1").is_none());
            assert!(match_change_id("qpvuntsm_").is_none());
        }

        #[test]
        fn match_bookmark_rejects_empty_or_unclosed() {
            assert!(match_bookmark("[]").is_none());
            assert!(match_bookmark("[main").is_none());
            assert_eq!(match_bookmark("[main]rest").unwrap(), 6);
        }

        #[test]
        fn match_empty_marker_variants() {
            assert_eq!(match_empty_marker("(empty)").unwrap(), 7);
            assert_eq!(match_empty_marker("(no description set)").unwrap(), 20);
            assert!(match_empty_marker("(other)").is_none());
        }

        #[test]
        fn match_file_status_prefix_accepts_tab() {
            assert_eq!(match_file_status_prefix("M\tfoo").unwrap(), 2);
        }

        #[test]
        fn match_file_status_prefix_rejects_non_flag() {
            assert!(match_file_status_prefix("X foo").is_none());
            assert!(match_file_status_prefix("M").is_none());
        }
    }

    #[test]
    fn token_types_and_modifiers_match_indices() {
        // Index assertions guard against the legend drifting from the const
        // indices used in the scanner.
        assert_eq!(TOKEN_TYPES[TT_COMMENT as usize], SemanticTokenType::COMMENT);
        assert_eq!(TOKEN_TYPES[TT_KEYWORD as usize], SemanticTokenType::KEYWORD);
        assert_eq!(TOKEN_TYPES[TT_STRING as usize], SemanticTokenType::STRING);
        assert_eq!(TOKEN_TYPES[TT_TYPE as usize], SemanticTokenType::TYPE);
        assert_eq!(
            TOKEN_TYPES[TT_ENUM_MEMBER as usize],
            SemanticTokenType::ENUM_MEMBER
        );
        assert_eq!(TOKEN_TYPES[TT_NUMBER as usize], SemanticTokenType::NUMBER);
        assert_eq!(
            TOKEN_TYPES[TT_OPERATOR as usize],
            SemanticTokenType::OPERATOR
        );
        assert_eq!(TOKEN_MODIFIERS[0], SemanticTokenModifier::DOCUMENTATION);
    }
}
