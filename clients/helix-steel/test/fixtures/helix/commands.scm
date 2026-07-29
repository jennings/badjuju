;; Stub of `helix/commands.scm` (typed commands) — see editor.scm's header.
(provide open new buffer_close code_action goto_definition)
(define (open path) (list 'OPEN path))
(define (new) (list 'NEW))
(define (buffer_close) (list 'BUFFER-CLOSE))
(define (code_action) (list 'CODE-ACTION))
(define (goto_definition) (list 'GOTO-DEFINITION))
