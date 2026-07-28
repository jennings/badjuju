;; Bad Juju — pure logic shared by badjuju.scm and unit-tested directly
;; (see clients/helix-steel/test/badjuju-test.scm) against a plain `steel`
;; interpreter. No `(require "helix/...")` here — anything that touches a
;; live editor context belongs in badjuju.scm instead.

(provide jj-window-kind jj-shortcut-line? jj-uri->path jj-cursor-arg-for)

;;@doc
;; Classify a badjuju buffer path into the window kind the server's keymap
;; profile (server/src/keymap.rs) and this plugin's dispatch tables key off
;; of. Mirrors the filename conventions in commands.rs / README.md.
(define (jj-window-kind path)
  (cond
    [(not (string? path)) 'other]
    [(ends-with? path "/status.jujutsu") 'status]
    [(ends-with? path "/log.jujutsu") 'log]
    [(ends-with? path "/describe.jujutsu") 'describe]
    [(ends-with? path "hunk-edit.jujutsu") 'hunk-edit]
    [(string-contains? path "/badjuju/squash/") 'squash]
    [(string-contains? path "/badjuju/file/") 'log-file]
    [(string-contains? path "/diff-change-") 'diff]
    [(string-contains? path "/diff-commit-") 'diff]
    [else 'other]))

;;@doc
;; #t when `line` is a `JJ: <Label>: <revset>` shortcut line from the log
;; buffer header (server/src/commands.rs::render_log_shortcuts). Requires a
;; colon after the "JJ: " prefix so plain "JJ: some note" text doesn't match.
(define (jj-shortcut-line? line)
  (define prefix "JJ: ")
  (define plen (string-length prefix))
  (and (string? line)
       (>= (string-length line) plen)
       (equal? (substring line 0 plen) prefix)
       (string-contains? (substring line plen (string-length line)) ":")))

;;@doc
;; Strip a `file://` prefix, passing non-file URIs (and plain paths) through
;; unchanged. badjuju always returns `file://` URIs to file-based clients.
(define (jj-uri->path uri)
  (define prefix "file://")
  (define plen (string-length prefix))
  (if (and (string? uri)
           (>= (string-length uri) plen)
           (equal? (substring uri 0 plen) prefix))
      (substring uri plen (string-length uri))
      uri))

;;@doc
;; Build the `{cursor:{uri,line}}` argument badjuju's cursor-form commands
;; expect (server/src/commands.rs::parse_cursor_arg), given an already-
;; resolved absolute path and 0-indexed line number.
(define (jj-cursor-arg-for path line)
  (hash "cursor" (hash "uri" (string-append "file://" path) "line" line)))
