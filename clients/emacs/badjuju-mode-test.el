;;; badjuju-mode-test.el --- Tests for badjuju-mode.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-mode)

;;; Mode hierarchy

(ert-deftest badjuju-mode-test/all-derive-from-parent ()
  (dolist (m '(badjuju-status-mode
               badjuju-log-mode
               badjuju-diff-mode
               badjuju-squash-mode
               badjuju-hunk-edit-mode
               badjuju-describe-mode))
    (should (provided-mode-derived-p m 'badjuju-mode))))

;;; buffer-read-only invariants

(ert-deftest badjuju-mode-test/status-readonly ()
  (with-temp-buffer
    (badjuju-status-mode)
    (should buffer-read-only)))

(ert-deftest badjuju-mode-test/diff-readonly ()
  (with-temp-buffer
    (badjuju-diff-mode)
    (should buffer-read-only)))

(ert-deftest badjuju-mode-test/squash-readonly ()
  (with-temp-buffer
    (badjuju-squash-mode)
    (should buffer-read-only)))

(ert-deftest badjuju-mode-test/log-writable ()
  (with-temp-buffer
    (badjuju-log-mode)
    (should-not buffer-read-only)))

(ert-deftest badjuju-mode-test/hunk-edit-writable ()
  (with-temp-buffer
    (badjuju-hunk-edit-mode)
    (should-not buffer-read-only)))

(ert-deftest badjuju-mode-test/describe-writable ()
  (with-temp-buffer
    (badjuju-describe-mode)
    (should-not buffer-read-only)))

;;; auto-mode-alist precedence (regression net for #35)
;;
;; The MOST-specific patterns are prepended last, so they sit earliest in
;; the list and are matched first.  Every concrete `.jj/badjuju/*.jujutsu'
;; path must resolve to its specific mode, not to the catch-all
;; `badjuju-mode'.

(defun badjuju-mode-test--match-auto-mode (path)
  "Return the mode that `auto-mode-alist' would select for PATH."
  (let ((case-fold-search nil))
    (assoc-default path auto-mode-alist #'string-match)))

(ert-deftest badjuju-mode-test/auto-mode-status ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/status.jujutsu")
              'badjuju-status-mode)))

(ert-deftest badjuju-mode-test/auto-mode-log ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/log.jujutsu")
              'badjuju-log-mode)))

(ert-deftest badjuju-mode-test/auto-mode-diff-change ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/diff-change-abc123.jujutsu")
              'badjuju-diff-mode)))

(ert-deftest badjuju-mode-test/auto-mode-diff-commit ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/diff-commit-abc123.jujutsu")
              'badjuju-diff-mode)))

(ert-deftest badjuju-mode-test/auto-mode-describe ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/describe.jujutsu")
              'badjuju-describe-mode)))

(ert-deftest badjuju-mode-test/auto-mode-hunk-edit ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/hunk-edit.jujutsu")
              'badjuju-hunk-edit-mode)))

(ert-deftest badjuju-mode-test/auto-mode-squash ()
  (should (eq (badjuju-mode-test--match-auto-mode
               "/repo/.jj/badjuju/squash/foo.jujutsu")
              'badjuju-squash-mode)))

(ert-deftest badjuju-mode-test/auto-mode-generic-jujutsu ()
  "Bare *.jujutsu (no `.jj/badjuju/' prefix) falls through to parent mode."
  (should (eq (badjuju-mode-test--match-auto-mode "/tmp/anything.jujutsu")
              'badjuju-mode)))

;;; describe-finish / describe-abort

(ert-deftest badjuju-mode-test/describe-finish-saves-and-buries ()
  (let (saved buried)
    (cl-letf (((symbol-function 'save-buffer)
               (lambda () (setq saved t)))
              ((symbol-function 'bury-buffer)
               (lambda () (setq buried t))))
      (badjuju-describe-finish))
    (should saved)
    (should buried)))

(ert-deftest badjuju-mode-test/describe-abort-clears-modified ()
  (let (buried)
    (with-temp-buffer
      (badjuju-describe-mode)
      (insert "edited")
      (set-buffer-modified-p t)
      (cl-letf (((symbol-function 'bury-buffer)
                 (lambda () (setq buried t))))
        (badjuju-describe-abort))
      (should-not (buffer-modified-p))
      (should buried))))

;;; refresh-virtual-buffer with point-clamp

(ert-deftest badjuju-mode-test/refresh-virtual-clamps-point ()
  (let* ((uri "badjuju-diff:///change/abc"))
    (with-temp-buffer
      (rename-buffer uri t)
      (badjuju-diff-mode)
      (let ((inhibit-read-only t))
        (insert "very long initial content with many characters")
        ;; place point near the end (beyond what new content can contain)
        (goto-char (point-max)))
      (cl-letf (((symbol-function 'eglot-current-server)
                 (lambda () 'mock))
                ((symbol-function 'jsonrpc-request)
                 (lambda (&rest _) (list :text "tiny"))))
        (badjuju--refresh-virtual-buffer uri))
      (should (equal (buffer-string) "tiny"))
      ;; Point must be clamped to (point-max).
      (should (= (point) (point-max))))))

(ert-deftest badjuju-mode-test/refresh-virtual-no-server-noop ()
  (let ((uri "badjuju-diff:///change/missing"))
    (with-temp-buffer
      (rename-buffer uri t)
      (badjuju-diff-mode)
      (cl-letf (((symbol-function 'eglot-current-server)
                 (lambda () nil)))
        ;; Should not error.
        (badjuju--refresh-virtual-buffer uri)))))

(provide 'badjuju-mode-test)
;;; badjuju-mode-test.el ends here
