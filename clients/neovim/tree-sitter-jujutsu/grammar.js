/**
 * @file Tree-sitter grammar for Jujutsu bad-juju buffers (status.jujutsu,
 *       log.jujutsu, describe.jujutsu, diff.jujutsu).
 *
 * Placeholder grammar: matches any input as a flat source_file. Subsequent
 * tickets (bad-juju-4b2, -tzm, -uvp) layer on real rules (comments, section
 * headers, file status markers, commit/change ids, graph chars).
 */

module.exports = grammar({
  name: "jujutsu",

  // Disable automatic whitespace skipping so the parser sees every byte.
  extras: () => [],

  rules: {
    source_file: ($) => repeat($._any),
    _any: () => /[\s\S]/,
  },
});
