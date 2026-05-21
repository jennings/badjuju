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
 *   3  jj_comment              (must beat section_keyword and content_text)
 *   2  section_keyword, file_status   (must beat content_text)
 *   0  content_text catch-all
 *
 * section_keyword embeds the trailing colon so the lexer only fires this
 * rule when the line really is a header; the alternative — a bare
 * [A-Z][A-Z ]* token — would also fire for English words like "Apple" and
 * leave the parser unable to find the required colon.
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

    _content_atom: ($) => choice($.empty_marker, $.bookmark, $._content_text),

    // (empty) and (no description set) markers, anywhere within a line.
    empty_marker: () => /\((empty|no description set)\)/,

    // [non-empty bookmark text] — bracketed identifier, anywhere within a
    // line. Tree-sitter has no lookbehind for "start of word" so any [..]
    // sequence not containing ] or newline is treated as a bookmark.
    bookmark: () => /\[[^\]\n]+\]/,

    // Greedy run of "uninteresting" characters, or a lone ( or [ that
    // failed to start an empty_marker / bookmark. Keeping the lone-char
    // alternatives last means we only fall to them when the structured
    // alternatives didn't match.
    // Use \x5b for `[` inside the negated class because the regex crate
    // tree-sitter ships rejects an unescaped `[` there, while biome's lint
    // rejects the conventional `\[` escape as "useless".
    _content_text: () => /[^\x5b(\n]+|\(|\[/,
  },
});
