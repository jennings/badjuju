# tree-sitter-jujutsu

Tree-sitter grammar for the Jujutsu (`bad-juju`) buffer formats — `status.jujutsu`, `log.jujutsu`, `describe.jujutsu`, and `diff.jujutsu`. Generated `src/parser.c` is committed so `:TSInstall` and `nvim-treesitter` consumers can pick it up without running the tree-sitter CLI. To regenerate after editing `grammar.js`, run `redo clients/neovim/tree-sitter-jujutsu/src/parser.c` (requires the `tree-sitter` CLI on `PATH`).
