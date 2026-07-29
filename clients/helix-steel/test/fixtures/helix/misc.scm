;; Stub of `helix/misc.scm` (send-lsp-command et al.) — see editor.scm's
;; header. Records every dispatched command instead of talking to a real LSP.
(provide send-lsp-command)
(define (send-lsp-command lsp-name method params callback)
  (list 'SENT lsp-name method params))
