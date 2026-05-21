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
 *   3  jj_comment              (must beat section_keyword + plain_line)
 *   2  section_keyword         (must beat plain_line)
 *   0  plain_line catch-all
 */

module.exports = grammar({
  name: "jujutsu",

  // Disable automatic whitespace skipping so the parser sees every byte and
  // can keep line boundaries authoritative.
  extras: () => [],

  rules: {
    source_file: ($) => repeat($._line),

    _line: ($) => choice($.jj_comment, $.section_header, $._plain_line),

    // ^JJ:.*$ — entire line is a comment.
    jj_comment: () => token(prec(3, /JJ:[^\n]*\n?/)),

    // ^([A-Z][A-Z ]*):(.*)$ — keyword and trailing exposed as named fields.
    section_header: ($) =>
      seq(
        field("header", $.section_keyword),
        ":",
        field("trailing", $.section_trailing),
        optional("\n"),
      ),

    section_keyword: () => token(prec(2, /[A-Z][A-Z ]*/)),
    section_trailing: () => /[^\n]*/,

    _plain_line: () => /[^\n]*\n?/,
  },
});
