;;; test-runner.el --- Headless ERT runner for badjuju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Entry point for `emacs --batch -L . -l test-runner.el
;; -f ert-run-tests-batch-and-exit'.
;;
;; Loads every sibling `*-test.el' file plus `test-e2e.el' if the
;; `BADJUJU_E2E' environment variable is set (e2e tests boot the real
;; badjuju binary against a tempdir jj repo and are slower than the
;; unit suite).

;;; Code:

(require 'ert)

(let ((dir (file-name-directory (or load-file-name buffer-file-name))))
  (add-to-list 'load-path dir)
  (load (expand-file-name "test-helpers.el" dir) nil t)
  ;; Load all unit-test files in deterministic order.
  (dolist (f (sort (directory-files dir t "-test\\.el\\'") #'string<))
    (load f nil t))
  ;; E2E suite is opt-in via env var.
  (when (and (getenv "BADJUJU_E2E")
             (file-exists-p (expand-file-name "test-e2e.el" dir)))
    (load (expand-file-name "test-e2e.el" dir) nil t)))

(provide 'test-runner)
;;; test-runner.el ends here
