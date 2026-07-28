;; Exercises every exported badjuju.scm command against the stub `helix/*`
;; modules in fixtures/helix/ (run.sh points `steel`'s module search path at
;; this directory). Doesn't prove editor behavior — real Helix-context calls
;; (editor-focus, rope ops, keymap registration) are stubbed out — only that
;; the file loads, every `provide`d name resolves, and every command runs
;; without an arity/undefined-identifier error, across all the window kinds
;; the dispatch logic branches on (status/log/squash/other).
;;
;; Run via ./run.sh, not directly — run.sh neutralizes the two
;; `require-builtin` lines (helix/core/text, helix/core/keymaps) that only
;; resolve inside a real Steel-Helix engine and can't be file-stubbed.

(require "cogs/badjuju.scm")

(define (check label actual expected)
  (unless (equal? actual expected)
    (displayln (list 'FAIL label 'got actual 'expected expected))
    (error "smoke test assertion failed")))

;; --- pure logic, sanity only (full coverage lives in badjuju-test.scm) ---
(check "window-kind status" (jj-window-kind "/repo/.jj/badjuju/status.jujutsu") 'status)
(check "shortcut-line?" (jj-shortcut-line? "JJ: Mutable:  x") #t)

;; --- every exported command must be callable without an arity/name error ---
(jj-status)
(jj-log)
(jj-log "author(me)")
(jj-log-file)
(jj-log-file "..@")
(jj-describe)
(jj-describe "abc123")
(jj-diff)
(jj-diff "abc123")
(jj-diff-commit)
(jj-new)
(jj-new "abc123")
(jj-next)
(jj-next #t)
(jj-prev)
(jj-prev #t)
(jj-refresh)
(jj-squash)
(jj-squash-commit)
(jj-squash-toggle)
(jj-squash-edit-hunk)
(jj-squash-select-all)
(jj-squash-select-none)
(jj-unsquash)
(jj-undo)
(jj-abandon)
(jj-abandon "abc123")
(jj-edit)
(jj-edit "abc123")
(jj-fetch)
(jj-push)
(jj-push-force)
(jj-rebase-onto)
(jj-rebase-after)
(jj-rebase-before)
(jj-cancel)
(jj-bookmark-create "main")
(jj-bookmark-move "main")
(jj-bookmark-delete "main")
(jj-bookmark-track "main@origin")
(jj-bookmark-forget "main")
(jj-help)
(jj-help "status")
(jj-keymap)
(jj-version)
(jj-ret)
(jj-code-action)
(jj-key-s)
(jj-key-a)
(jj-key-u)
(jj-install-keymap!)

(displayln (list "badjuju.scm: smoke test passed, window kind:" (jj-window-kind (env-var "BADJUJU_TEST_PATH"))))
