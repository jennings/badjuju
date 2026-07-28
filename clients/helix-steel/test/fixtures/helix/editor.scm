;; Stub of the real `helix/editor.scm` module (registered by the Steel-Helix
;; runtime; see helix-term/src/commands/engine/steel/mod.rs). Used only to
;; smoke-test that badjuju.scm loads and every exported command is callable
;; against a plain `steel` interpreter — see ../smoke.scm and ../run.sh.
(provide editor-focus editor->doc-id editor-document->path editor->text set-scratch-buffer-name!)
(define (editor-focus) 'stub-view)
(define (editor->doc-id view) 'stub-doc)
;; BADJUJU_TEST_PATH lets run.sh exercise the window-kind dispatch branches
;; (status/log/squash/other) without duplicating this whole fixture tree.
(define (editor-document->path doc-id)
  (let ([p (env-var "BADJUJU_TEST_PATH")]) (if (equal? p "") "/repo/.jj/badjuju/status.jujutsu" p)))
(define (editor->text doc-id) 'stub-rope)
(define (set-scratch-buffer-name! name) (list 'SET-SCRATCH-NAME name))
