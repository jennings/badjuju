;; Bad Juju — Steel plugin for Helix.
;;
;; Requires a Steel-enabled Helix build (helix-term built with `--features
;; steel`; see clients/helix-steel/README.md). Vanilla Helix cannot load this
;; file — its `(require "helix/...")` modules don't exist without the
;; `steel` Cargo feature.
;;
;; This plugin exists to close two gaps the plain LSP-only setup
;; (clients/helix/languages.toml) can't close on its own:
;;
;; 1. Helix's built-in `code_action` popup sends workspace/executeCommand and
;;    discards the JSON-RPC result (helix-view's `execute_lsp_command` only
;;    logs errors — see the doc comment on `jj-open-result!` below). badjuju
;;    returns the URI of the file a command just wrote, so every state-
;;    changing action needs a client that opens that URI. `jj-execute!` does
;;    that; every `jj-*` command below is built on it.
;; 2. Helix has no keybinding layer of its own for `*.jujutsu` buffers (no
;;    "buffer-local keymap" concept without Steel), so `Space a` was the only
;;    way to invoke any badjuju action, and RET couldn't be context-dispatched
;;    the way it is in Neovim/Emacs (apply a `JJ:` revset shortcut vs. goto
;;    definition). `jj-ret` and `set-global-buffer-or-extension-keymap` below
;;    close that gap.
;;
;; Everything else (syntax highlighting, semantic tokens, diagnostics,
;; goto-definition on commit/file rows, auto-reload via workspace/applyEdit)
;; already works over plain LSP and needs no Steel code.

(require "helix/editor.scm")
(require "helix/misc.scm")
(require (prefix-in helix. "helix/commands.scm"))
(require (prefix-in helix.static. "helix/static.scm"))
(require-builtin helix/core/text)
(require "badjuju-core.scm")

(provide jj-status
         jj-log
         jj-log-file
         jj-describe
         jj-diff
         jj-diff-commit
         jj-new
         jj-next
         jj-prev
         jj-refresh
         jj-squash
         jj-squash-commit
         jj-squash-toggle
         jj-squash-edit-hunk
         jj-squash-select-all
         jj-squash-select-none
         jj-unsquash
         jj-undo
         jj-abandon
         jj-edit
         jj-fetch
         jj-push
         jj-push-force
         jj-rebase-source
         jj-rebase-onto
         jj-rebase-after
         jj-rebase-before
         jj-cancel
         jj-bookmark-create
         jj-bookmark-move
         jj-bookmark-delete
         jj-bookmark-track
         jj-bookmark-forget
         jj-help
         jj-keymap
         jj-version
         jj-ret
         jj-code-action
         jj-window-kind
         jj-shortcut-line?
         jj-uri->path
         jj-cursor-arg-for
         jj-install-keymap!)

;; Pure helpers (jj-window-kind, jj-shortcut-line?, jj-uri->path,
;; jj-cursor-arg-for) live in badjuju-core.scm, which has no editor-context
;; dependency and is unit-tested directly against a plain `steel`
;; interpreter (see test/badjuju-test.scm). Re-provided here so callers only
;; need to require this one file.
;;
;; ---------------------------------------------------------------------------
;; Editor-context helpers (require a live Steel-Helix runtime; not unit-
;; tested — exercised only by hand against a Steel-enabled `hx`).
;; ---------------------------------------------------------------------------

(define (jj-current-path)
  (editor-document->path (editor->doc-id (editor-focus))))

(define (jj-current-line)
  (let* ([doc-id (editor->doc-id (editor-focus))]
         [text (editor->text doc-id)]
         [range (helix.static.selection->primary-range (helix.static.current-selection-object))]
         [char-idx (helix.static.range->from range)])
    (rope-char->line text char-idx)))

(define (jj-current-line-text)
  (let* ([doc-id (editor->doc-id (editor-focus))]
         [text (editor->text doc-id)])
    (rope->string (rope->line text (jj-current-line)))))

(define (jj-window-kind-here)
  (jj-window-kind (jj-current-path)))

(define (jj-cursor-arg)
  (jj-cursor-arg-for (jj-current-path) (jj-current-line)))

;; ---------------------------------------------------------------------------
;; Dispatch core
;; ---------------------------------------------------------------------------

;; Must match the `name` badjuju is registered under in languages.toml
;; ([language-server.badjuju]) — Steel's send-lsp-command looks servers up by
;; that name, not by language id.
(define BADJUJU-LSP "badjuju")

;;@doc
;; Open the file at a badjuju command's result URI, unless it's already the
;; focused buffer. This is the piece vanilla Helix code actions can't do:
;; helix-view's `Editor::execute_lsp_command` fires workspace/executeCommand
;; and discards the response ("the command is executed on the server and
;; communicated back to the client asynchronously using workspace edits") —
;; but badjuju communicates back by *returning* the URI of the file it just
;; wrote, which only a caller that reads the JSON-RPC result can act on.
;; Buffers that are already open get their content refreshed independently
;; via workspace/applyEdit (server/src/server.rs::apply_edit_if_open), so
;; this only needs to handle a *different* file coming back (a freshly
;; written describe.jujutsu, diff window, or squash window).
(define (jj-open-result! result)
  (when (and (string? result) (> (string-length result) 0))
    (define path (jj-uri->path result))
    (unless (equal? path (jj-current-path))
      (helix.open path))))

;;@doc
;; Send a badjuju workspace command and open whatever file URI it returns.
(define (jj-execute! command args)
  (send-lsp-command BADJUJU-LSP
                     "workspace/executeCommand"
                     (hash "command" command "arguments" args)
                     jj-open-result!))

;;@doc
;; Send a badjuju workspace command and hand the raw deserialized result to
;; `cb`, for commands that return structured data instead of a file URI
;; (badjuju.help, badjuju.keymap, badjuju.version).
(define (jj-request! command args cb)
  (send-lsp-command BADJUJU-LSP "workspace/executeCommand" (hash "command" command "arguments" args) cb))

;; Resolve the revision argument shared by describe/diff/diff-commit/new/
;; abandon/edit: an explicit literal revision if the caller passed one,
;; cursor-form when invoked from a status/log row, otherwise none (server
;; defaults to `@`).
(define (jj-revision-args explicit)
  (cond
    [(pair? explicit) (list (car explicit))]
    [(memq (jj-window-kind-here) '(status log)) (list (jj-cursor-arg))]
    [else '()]))

;; ---------------------------------------------------------------------------
;; Commands — one per badjuju.* server command (server/src/server.rs::COMMANDS)
;; ---------------------------------------------------------------------------

;;@doc
;; Open the working-copy status buffer.
(define (jj-status) (jj-execute! "badjuju.status" '()))

;;@doc
;; Open the commit log. Optional revset argument: `(jj-log "author(me)")`.
;; With no argument and the cursor on a `JJ:` shortcut line in an already-open
;; log buffer, re-runs the log with that shortcut's revset (mirrors RET in
;; Neovim/Emacs); otherwise uses the server's default revset.
(define (jj-log . revset)
  (cond
    [(pair? revset) (jj-execute! "badjuju.log" (list (car revset)))]
    [(and (eq? (jj-window-kind-here) 'log) (jj-shortcut-line? (jj-current-line-text)))
     (jj-execute! "badjuju.log" (list (jj-cursor-arg)))]
    [else (jj-execute! "badjuju.log" '())]))

;;@doc
;; Open the per-file history for the file at cursor (status buffer) or the
;; current buffer's file, with an optional revset (default `..@`).
(define (jj-log-file . revset)
  (define path
    (if (eq? (jj-window-kind-here) 'status)
        (jj-cursor-arg)
        (jj-current-path)))
  (jj-execute! "badjuju.log.file" (list path (if (pair? revset) (car revset) ""))))

;;@doc
;; Open describe.jujutsu for the revision at cursor (or given revision).
(define (jj-describe . revision) (jj-execute! "badjuju.describe" (jj-revision-args revision)))

;;@doc
;; Open the change diff (updates on amend) for the revision at cursor (or given revision).
(define (jj-diff . revision) (jj-execute! "badjuju.diff" (jj-revision-args revision)))

;;@doc
;; Open a pinned commit diff (never refreshed) for the revision at cursor (or given revision).
(define (jj-diff-commit . revision) (jj-execute! "badjuju.diff.commit" (jj-revision-args revision)))

;;@doc
;; Create a new change (child of the revision at cursor in status/log, or of `@`).
(define (jj-new . parent) (jj-execute! "badjuju.new" (jj-revision-args parent)))

;;@doc
;; Move @ to the next child. Non-#f argument requests --edit.
(define (jj-next . edit) (jj-execute! "badjuju.next" (list (and (pair? edit) (car edit)))))

;;@doc
;; Move @ to the previous parent. Non-#f argument requests --edit.
(define (jj-prev . edit) (jj-execute! "badjuju.prev" (list (and (pair? edit) (car edit)))))

;;@doc
;; Refresh the current badjuju buffer (or status, from a non-badjuju buffer).
(define (jj-refresh)
  (define path (jj-current-path))
  (jj-execute! "badjuju.refresh"
               (list (if (eq? (jj-window-kind path) 'other) "" (string-append "file://" path)))))

;;@doc
;; Squash the file (or working-copy line) at cursor into its parent.
(define (jj-squash) (jj-execute! "badjuju.squash" (list (jj-cursor-arg))))

;;@doc
;; Two-step commit-to-commit squash: first call marks the revision at cursor
;; as the source, second call (on a different revision) squashes into it and
;; opens the resulting squash window.
(define (jj-squash-commit) (jj-execute! "badjuju.squash.commit" (list (jj-cursor-arg))))

;;@doc
;; In a squash window: toggle the hunk or file at cursor between REMAINING and SELECTED.
(define (jj-squash-toggle) (jj-execute! "badjuju.squash.toggle" (list (jj-cursor-arg))))

;;@doc
;; In a squash window: open the hunk-edit buffer for the hunk at cursor.
(define (jj-squash-edit-hunk) (jj-execute! "badjuju.squash.edit_hunk" (list (jj-cursor-arg))))

;;@doc
;; In a squash window: move every remaining hunk to SELECTED.
(define (jj-squash-select-all) (jj-execute! "badjuju.squash.select_all" '()))

;;@doc
;; In a squash window: move every selected hunk back to REMAINING.
(define (jj-squash-select-none) (jj-execute! "badjuju.squash.select_none" '()))

;;@doc
;; Unsquash the file at cursor from its revision's parent back into it.
(define (jj-unsquash) (jj-execute! "badjuju.unsquash" (list (jj-cursor-arg))))

;;@doc
;; Undo the last jj operation (`jj undo`).
(define (jj-undo) (jj-execute! "badjuju.undo" '()))

;;@doc
;; Abandon the revision at cursor (or given revision; defaults to `@`).
(define (jj-abandon . revision) (jj-execute! "badjuju.abandon" (jj-revision-args revision)))

;;@doc
;; Move @ to the revision at cursor (or given revision) — `jj edit`.
(define (jj-edit . revision) (jj-execute! "badjuju.edit" (jj-revision-args revision)))

;;@doc
;; Run `jj git fetch`.
(define (jj-fetch) (jj-execute! "badjuju.fetch" '()))

;;@doc
;; Run `jj git push`.
(define (jj-push) (jj-execute! "badjuju.push" (list (hash "forceWithLease" #f))))

;;@doc
;; Run `jj git push --force-with-lease`.
(define (jj-push-force) (jj-execute! "badjuju.push" (list (hash "forceWithLease" #t))))

;; Shared rebase-source helper: mode is "source", "revisions", or "branch".
(define (jj-rebase-source mode) (jj-execute! "badjuju.rebase.source" (list mode (jj-cursor-arg))))

;;@doc
;; Complete a pending rebase: insert the source onto the revision at cursor.
(define (jj-rebase-onto) (jj-execute! "badjuju.rebase.commit" (list "onto" (jj-cursor-arg))))

;;@doc
;; Complete a pending rebase: insert the source after the revision at cursor.
(define (jj-rebase-after) (jj-execute! "badjuju.rebase.commit" (list "after" (jj-cursor-arg))))

;;@doc
;; Complete a pending rebase: insert the source before the revision at cursor.
(define (jj-rebase-before) (jj-execute! "badjuju.rebase.commit" (list "before" (jj-cursor-arg))))

;;@doc
;; Cancel any pending squash or rebase operation.
(define (jj-cancel) (jj-execute! "badjuju.cancel" (list (jj-cursor-arg))))

;; Shared bookmark helper.
(define (jj-bookmark sub-action name)
  (jj-execute! "badjuju.bookmark"
               (list sub-action
                     name
                     (if (memq (jj-window-kind-here) '(status log)) (jj-cursor-arg) ""))))

;;@doc
;; Create a bookmark named `name` at the revision at cursor (or `@`).
(define (jj-bookmark-create name) (jj-bookmark "create" name))

;;@doc
;; Move bookmark `name` to the revision at cursor (or `@`).
(define (jj-bookmark-move name) (jj-bookmark "move" name))

;;@doc
;; Delete bookmark `name`.
(define (jj-bookmark-delete name) (jj-bookmark "delete" name))

;;@doc
;; Track a remote bookmark, e.g. `(jj-bookmark-track "main@origin")`.
(define (jj-bookmark-track name) (jj-bookmark "track" name))

;;@doc
;; Forget bookmark `name`.
(define (jj-bookmark-forget name) (jj-bookmark "forget" name))

;; Render a JSON-object-shaped Steel hash as indented "key: value" lines.
(define (jj-format-hash h)
  (apply string-append
         (map (lambda (k) (string-append (to-string k) ": " (to-string (hash-ref h k)) "\n"))
              (hash-keys->list h))))

;; Open a fresh scratch buffer named `title` containing `text`.
(define (jj-show-scratch! title text)
  (helix.static.new)
  (set-scratch-buffer-name! title)
  (helix.static.insert_string text))

;;@doc
;; Show the active keymap profile and bindings in a scratch buffer.
(define (jj-keymap)
  (jj-request! "badjuju.keymap" '() (lambda (result) (jj-show-scratch! "*badjuju-keymap*" (jj-format-hash result)))))

;;@doc
;; Show the command reference for `window` ("status", "log", "diff", …;
;; defaults to the current buffer's window kind) in a scratch buffer.
(define (jj-help . window)
  (define w (if (pair? window) (car window) (symbol->string (jj-window-kind-here))))
  (jj-request! "badjuju.help"
               (list w)
               (lambda (result) (jj-show-scratch! "*badjuju-help*" (jj-format-hash result)))))

;;@doc
;; Show the badjuju server version in a scratch buffer.
(define (jj-version)
  (jj-request! "badjuju.version" '() (lambda (result) (jj-show-scratch! "*badjuju-version*" (jj-format-hash result)))))

;;@doc
;; Context-dispatched RET: on a `JJ:` shortcut line in log.jujutsu, apply the
;; revset (same as `jj-log` with no argument); everywhere else, goto
;; definition (commit/file rows navigate exactly as `gd` does over plain
;; LSP). Mirrors ret_dispatch in the Neovim/Emacs clients.
(define (jj-ret)
  (if (and (eq? (jj-window-kind-here) 'log) (jj-shortcut-line? (jj-current-line-text)))
      (jj-log)
      (helix.static.goto_definition)))

;;@doc
;; Run Helix's native code-action picker. Exposed as a named command so it
;; can be bound in the jujutsu keymap alongside jj-ret et al.
(define (jj-code-action) (helix.static.code_action))

(provide jj-key-s jj-key-a jj-key-u)

;;@doc
;; Dispatch for the tab-menu `s`: squash-commit (status/log) or squash-toggle
;; (squash window).
(define (jj-key-s)
  (case (jj-window-kind-here)
    [(squash) (jj-squash-toggle)]
    [(status log) (jj-squash-commit)]
    [else void]))

;;@doc
;; Dispatch for the tab-menu `a`: abandon revision at cursor (status/log) or
;; select-all (squash window).
(define (jj-key-a)
  (case (jj-window-kind-here)
    [(squash) (jj-squash-select-all)]
    [(status log) (jj-abandon)]
    [else void]))

;;@doc
;; Dispatch for the tab-menu `u`: unsquash file at cursor (status) or undo
;; (squash window).
(define (jj-key-u)
  (case (jj-window-kind-here)
    [(squash) (jj-undo)]
    [(status) (jj-unsquash)]
    [else void]))

;; ---------------------------------------------------------------------------
;; Keymap
;;
;; Two bindings only, both chosen to be collision-free with vanilla Helix
;; normal mode (verified against helix-term/src/keymap/default.rs):
;;
;; * `ret` — context dispatch (jj-ret). No default Helix normal-mode binding
;;   exists for bare Enter, so this is safe everywhere `.jujutsu` is open,
;;   including describe.jujutsu and hunk-edit.jujutsu (real text-entry
;;   buffers): it only *does* something (apply a `JJ:` revset shortcut) on an
;;   actual shortcut line in log.jujutsu, and falls back to goto_definition
;;   (a no-op on prose) everywhere else.
;; * `tab` — opens a "Bad Juju" sub-keymap (the same mechanism Helix's own
;;   `g`/`z`/`space`/`[`/`]` prefixes use — press it and Helix pops up a menu
;;   of the bound keys and their doc strings). `tab` has no normal-mode
;;   binding in vanilla Helix either (it's insert-mode-only, for
;;   `smart_tab`/`insert_tab`), so claiming it as a prefix costs nothing.
;;   Every magit-style single letter (n/l/d/D/e/s/S/a/u/U/f/p/P/R/x/?) lives
;;   *inside* this submenu instead of at the top level, so it can never
;;   shadow a bare-key Helix default or collide with prose editing in
;;   describe/hunk-edit — reaching any of them costs one extra keystroke
;;   (`tab` then the letter) in exchange for zero collision risk.
;;
;; This is why a single flat magit keymap (the kind Kakoune/Neovim/Emacs
;; ship, gated by a leader key or a true per-buffer-number local map) isn't
;; reproduced here: Steel's extension/label keymap is a static override for
;; every buffer with that extension, and badjuju's status/log/diff/squash/
;; describe/hunk-edit buffers all share the single `.jujutsu` extension —
;; there's no per-window-kind fallback to "native binding" the way a
;; per-buffer-number map has. Nesting everything but `ret` under `tab`
;; sidesteps that limitation entirely rather than working around it with
;; window-kind checks on every top-level letter.
;;
;; Registered directly against the `helix/core/keymaps` native module rather
;; than the `keymap` macro from `helix/keymaps.scm`, since the wire format
;; (a JSON string decoded straight into Rust's `KeyTrieNode`, per
;; helix-term/src/keymap.rs's `Deserialize` impl) is simpler to get
;; obviously-correct than the macro's hash-merging expansion. Leaf values
;; must be `:`-prefixed and match a name `provide`d from your `helix.scm` —
;; see the README for the required `(require "cogs/badjuju.scm")` /
;; `(provide jj-status jj-new ...)` wiring, without which every binding
;; below resolves to "no such command" at press time, not a load-time error.
;; ---------------------------------------------------------------------------

(require-builtin helix/core/keymaps as helix.keymaps.)

(define (jj-keymap-json)
  (value->jsexpr-string
   (hash "normal"
         (hash "ret" ":jj-ret"
               "tab"
               (hash "label" "Bad Juju"
                     "n" ":jj-new"
                     "l" ":jj-log"
                     "L" ":jj-log-file"
                     "d" ":jj-diff"
                     "D" ":jj-diff-commit"
                     "e" ":jj-edit"
                     "s" ":jj-key-s"
                     "S" ":jj-squash"
                     "a" ":jj-key-a"
                     "u" ":jj-key-u"
                     "U" ":jj-undo"
                     "f" ":jj-fetch"
                     "p" ":jj-push"
                     "P" ":jj-push-force"
                     "R" ":jj-refresh"
                     "x" ":jj-cancel"
                     "q" ":buffer-close"
                     "A" ":jj-code-action"
                     "?" ":jj-help")))))

;;@doc
;; Install the jujutsu keymap (`ret` plus a `tab`-prefixed menu — see the
;; comment above this section for why nothing else is bound at the top
;; level). Call once from init.scm, after re-providing the jj-* commands
;; from your helix.scm (see README):
;;   (require "cogs/badjuju.scm")
;;   (jj-install-keymap!)
(define (jj-install-keymap!)
  (helix.keymaps.#%add-extension-or-labeled-keymap
   "jujutsu"
   (helix.keymaps.helix-string->keymap (jj-keymap-json))))
