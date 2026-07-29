;; Unit tests for the pure logic in cogs/badjuju-core.scm. Runs against a
;; plain `steel` interpreter (no Helix, no Steel-enabled `hx` build needed) —
;; see clients/helix-steel/test.do.
;;
;;   steel clients/helix-steel/test/badjuju-test.scm
;;
;; Every assertion uses assert! (panics with a non-zero exit code on
;; failure), so the whole file is a single pass/fail unit for `redo test`.

(require "../cogs/badjuju-core.scm")

;; --- jj-window-kind ----------------------------------------------------

(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/status.jujutsu") 'status))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/log.jujutsu") 'log))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/describe.jujutsu") 'describe))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/hunk-edit.jujutsu") 'hunk-edit))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/squash/abc123-def456.jujutsu") 'squash))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/file/src/main.rs.jujutsu") 'log-file))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/diff-change-abc123456789.jujutsu") 'diff))
(assert! (equal? (jj-window-kind "/repo/.jj/badjuju/diff-commit-abc123456789.jujutsu") 'diff))
(assert! (equal? (jj-window-kind "/repo/src/main.rs") 'other))
(assert! (equal? (jj-window-kind #f) 'other))
;; A path that merely contains "status.jujutsu" mid-string (not as the
;; basename) must not be misclassified — only a trailing "/status.jujutsu"
;; counts.
(assert! (equal? (jj-window-kind "/repo/status.jujutsu.bak") 'other))

;; --- jj-shortcut-line? ---------------------------------------------------

(assert! (jj-shortcut-line? "JJ: Mutable:  ancestors(reachable(@, mutable()), 2)"))
(assert! (jj-shortcut-line? "JJ: Stack:    (immutable_heads()..@)::"))
;; No colon after the label -> not a shortcut line (avoids matching
;; incidental "JJ: " prose the server might ever emit).
(assert! (not (jj-shortcut-line? "JJ: just some prose with no second colon")))
;; Doesn't start with the "JJ: " prefix at all.
(assert! (not (jj-shortcut-line? "@  kpkzwvqm 909679d0 1min stephen@example.com")))
;; Empty / short lines don't crash the length check.
(assert! (not (jj-shortcut-line? "")))
(assert! (not (jj-shortcut-line? "JJ")))
(assert! (not (jj-shortcut-line? #f)))

;; --- jj-uri->path --------------------------------------------------------

(assert! (equal? (jj-uri->path "file:///repo/.jj/badjuju/status.jujutsu")
                  "/repo/.jj/badjuju/status.jujutsu"))
;; Non-file URIs (badjuju's virtual-diff scheme, used by other clients) pass
;; through unchanged rather than getting mangled.
(assert! (equal? (jj-uri->path "badjuju-diff:///change/abc123") "badjuju-diff:///change/abc123"))
;; A bare path with no scheme also passes through unchanged.
(assert! (equal? (jj-uri->path "/already/a/path") "/already/a/path"))
;; Empty string doesn't crash the prefix-length check.
(assert! (equal? (jj-uri->path "") ""))

;; --- jj-cursor-arg-for ---------------------------------------------------

(define cursor-arg (jj-cursor-arg-for "/repo/.jj/badjuju/log.jujutsu" 7))
(assert! (hash? cursor-arg))
(define cursor-inner (hash-ref cursor-arg "cursor"))
(assert! (equal? (hash-ref cursor-inner "uri") "file:///repo/.jj/badjuju/log.jujutsu"))
(assert! (equal? (hash-ref cursor-inner "line") 7))

(displayln "badjuju-core.scm: all assertions passed")
