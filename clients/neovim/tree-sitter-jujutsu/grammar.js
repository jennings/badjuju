/**
 * @file Tree-sitter grammar for Jujutsu bad-juju buffers (status.jujutsu,
 *       log.jujutsu, describe.jujutsu, diff.jujutsu).
 *
 * The grammar is line-oriented: each top-level production matches a full
 * line (including its trailing newline when present) so that line-anchored
 * patterns like ^JJ:.*$ are enforced by structure rather than by anchors,
 * which tree-sitter regex tokens cannot express.
 *
 * Token precedence biases the lexer toward the structured productions when
 * an "any line" catch-all could otherwise win on length:
 *   3  jj_comment, commit_id   (must beat section_keyword / _alnum_run)
 *   2  section_keyword, file_status,
 *      change_id, graph_char, _alnum_run   (must beat content_text)
 *   0  content_text catch-all
 *
 * section_keyword embeds the trailing colon so the lexer only fires this
 * rule when the line really is a header; the alternative — a bare
 * [A-Z][A-Z ]* token — would also fire for English words like "Apple" and
 * leave the parser unable to find the required colon.
 *
 * change_id (8 lowercase letters) intentionally overlaps with common
 * English words. The TextMate grammar accepts the same overlap and lets
 * highlight precedence decide; we mirror that behavior here.
 */

module.exports = grammar({
  name: "jujutsu",

  // Disable automatic whitespace skipping so the parser sees every byte and
  // can keep line boundaries authoritative.
  extras: () => [],

  rules: {
    source_file: ($) => repeat($._line),

    _line: ($) =>
      choice(
        $.jj_comment,
        $.section_header,
        $.file_status_line,
        $._content_line,
        $._blank_line,
      ),

    // ^JJ:.*$ — entire line is a comment.
    jj_comment: () => token(prec(3, /JJ:[^\n]*\n?/)),

    // ^([A-Z][A-Z ]*):(.*)$ — keyword (with its colon) and trailing text
    // exposed as named fields so highlights.scm can color them separately.
    // prec.right ensures the optional trailing newline is bound to this line
    // rather than deferred to a subsequent _blank_line.
    section_header: ($) =>
      prec.right(
        seq(
          field("header", $.section_keyword),
          field("trailing", $.section_trailing),
          optional("\n"),
        ),
      ),
    section_keyword: () => token(prec(2, /[A-Z][A-Z ]*:/)),
    section_trailing: () => /[^\n]*/,

    // ^([AMDR]) <rest of line> — first char marks file status (jj status
    // output uses these letters for Added/Modified/Deleted/Renamed).
    file_status_line: ($) =>
      prec.right(seq($.file_status, repeat($._content_atom), optional("\n"))),
    file_status: () => token(prec(2, /[AMDR] /)),

    // A content line: at least one inline atom (empty_marker, bookmark, or
    // plain text) plus an optional trailing newline. prec.right keeps the
    // newline attached to the current line.
    _content_line: ($) =>
      prec.right(seq($._content_atom, repeat($._content_atom), optional("\n"))),

    _blank_line: () => "\n",

    _content_atom: ($) =>
      choice(
        $.empty_marker,
        $.bookmark,
        $.commit_id,
        $.change_id,
        $.graph_char,
        $._alnum_run,
        $._content_text,
      ),

    // (empty) and (no description set) markers, anywhere within a line.
    empty_marker: () => /\((empty|no description set)\)/,

    // [non-empty bookmark text] — bracketed identifier, anywhere within a
    // line. Tree-sitter has no lookbehind for "start of word" so any [..]
    // sequence not containing ] or newline is treated as a bookmark.
    bookmark: () => /\[[^\]\n]+\]/,

    // [0-9][0-9a-f]{7,39} — hex commit id, 8 to 40 chars total. Tree-sitter
    // tokens cannot contain regex assertions like \b, so word-boundary
    // behavior is approximated by _alnum_run below, which consumes any
    // longer lowercase-alphanumeric stretch and prevents change_id from
    // carving a prefix out of a longer identifier. commit_id has higher
    // precedence than _alnum_run so a pure hex run still binds as a
    // commit_id rather than a hidden alphanumeric blob.
    commit_id: () => token(prec(3, /[0-9][0-9a-f]{7,39}/)),

    // [a-z]{8} — exactly 8 lowercase letters. Overlap with common English
    // words is intentional; see the file-level comment. _alnum_run prevents
    // matching an 8-letter prefix of a longer lowercase run.
    change_id: () => token(prec(2, /[a-z]{8}/)),

    // Single graph drawing or working-copy marker character.
    graph_char: () => token(prec(2, /[│◆@~…○◉╭╮╯╰├─┤]/)),

    // Hidden helper: lowercase-alphanumeric runs of 9 or more characters.
    // Because tree-sitter prefers the longest match at equal precedence,
    // this rule out-competes change_id whenever the surrounding text would
    // have violated change_id's word-boundary expectation.
    _alnum_run: () => token(prec(2, /[a-z0-9]{9,}/)),

    // Filler text between recognized atoms. The first alternative
    // greedily eats characters that cannot start any other atom (uppercase
    // letters, punctuation, etc.); the second alternative matches any
    // remaining non-newline char so the lexer can advance one position at
    // a time through stretches that start with an "atom-leading" char
    // (lowercase letter, digit, `[`, `(`, graph char) without committing
    // to the wrong rule.
    //
    // Use \x5b for `[` inside the negated class because the regex crate
    // tree-sitter ships rejects an unescaped `[` there, while biome's
    // lint rejects the conventional `\[` escape as "useless".
    _content_text: () => /[^\x5b(\n0-9a-z│◆@~…○◉╭╮╯╰├─┤]+|[^\n]/,
  },
});
