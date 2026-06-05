;;; badjuju-keymap-test.el --- Tests for badjuju-keymap.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-keymap)

;;; Status & log keymap bindings (regression net for #47 u/U swap, #48)

(ert-deftest badjuju-keymap-test/status-u-is-unsquash ()
  "Lowercase `u' in status MUST be unsquash, not undo (regression for #47)."
  (should (eq (lookup-key badjuju-status-mode-map (kbd "u"))
              #'badjuju-unsquash)))

(ert-deftest badjuju-keymap-test/status-U-is-undo ()
  "Uppercase `U' in status MUST be undo, not unsquash (regression for #47)."
  (should (eq (lookup-key badjuju-status-mode-map (kbd "U"))
              #'badjuju-undo)))

(ert-deftest badjuju-keymap-test/status-RET-is-xref ()
  (should (eq (lookup-key badjuju-status-mode-map (kbd "RET"))
              #'xref-find-definitions)))

(ert-deftest badjuju-keymap-test/status-D-is-diff-commit ()
  (should (eq (lookup-key badjuju-status-mode-map (kbd "D"))
              #'badjuju-diff-commit)))

(ert-deftest badjuju-keymap-test/status-d-is-diff ()
  (should (eq (lookup-key badjuju-status-mode-map (kbd "d"))
              #'badjuju-diff)))

(ert-deftest badjuju-keymap-test/status-c-is-commit-transient ()
  (should (eq (lookup-key badjuju-status-mode-map (kbd "c"))
              #'badjuju-commit)))

(ert-deftest badjuju-keymap-test/log-RET-is-ret-dispatch ()
  "Log buffers use the RET dispatcher, not raw xref (#48)."
  (should (eq (lookup-key badjuju-log-mode-map (kbd "RET"))
              #'badjuju--ret-dispatch)))

(ert-deftest badjuju-keymap-test/log-U-is-undo ()
  (should (eq (lookup-key badjuju-log-mode-map (kbd "U"))
              #'badjuju-undo)))

(ert-deftest badjuju-keymap-test/diff-RET-is-xref ()
  (should (eq (lookup-key badjuju-diff-mode-map (kbd "RET"))
              #'xref-find-definitions)))

(ert-deftest badjuju-keymap-test/diff-q-buries ()
  (should (eq (lookup-key badjuju-diff-mode-map (kbd "q"))
              #'bury-buffer)))

(ert-deftest badjuju-keymap-test/squash-u-is-undo ()
  (should (eq (lookup-key badjuju-squash-mode-map (kbd "u"))
              #'badjuju-undo)))

(ert-deftest badjuju-keymap-test/hunk-edit-C-c-C-c ()
  (should (eq (lookup-key badjuju-hunk-edit-mode-map (kbd "C-c C-c"))
              #'save-buffer)))

(ert-deftest badjuju-keymap-test/active-buffer-sees-status-bindings ()
  "Activating `badjuju-status-mode' must install its keymap.
Regression net: populating the existing map (rather than rebinding the
defvar) is the only reason the bindings survive `define-derived-mode'."
  (with-temp-buffer
    (badjuju-status-mode)
    (should (eq (key-binding (kbd "u")) #'badjuju-unsquash))
    (should (eq (key-binding (kbd "U")) #'badjuju-undo))
    (should (eq (key-binding (kbd "d")) #'badjuju-diff))))

;;; RET dispatch regex

(ert-deftest badjuju-keymap-test/ret-dispatch-revset-line-runs-log ()
  (badjuju-test--with-captured-run
    (with-temp-buffer
      (insert "JJ: Trunk: main\n")
      (goto-char (point-min))
      (setq buffer-file-name "/tmp/x.jujutsu")
      (badjuju-log-mode)
      (badjuju--ret-dispatch)))
  (should (equal (caar badjuju-test-calls) "badjuju.log")))

(ert-deftest badjuju-keymap-test/ret-dispatch-non-shortcut-calls-xref ()
  (let (xref-called)
    (cl-letf (((symbol-function 'call-interactively)
               (lambda (fn) (when (eq fn #'xref-find-definitions)
                              (setq xref-called t)))))
      (with-temp-buffer
        (insert "some other line\n")
        (goto-char (point-min))
        (badjuju--ret-dispatch))
      (should xref-called))))

;;; Squash file and cancel

(ert-deftest badjuju-keymap-test/cancel-calls-badjuju-cancel ()
  (badjuju-test--with-captured-run
    (with-temp-buffer
      (setq buffer-file-name "/tmp/x.jujutsu")
      (badjuju-status-mode)
      (badjuju--run-cancel)))
  (should (equal (caar badjuju-test-calls) "badjuju.cancel"))
  (let ((args (cadar badjuju-test-calls)))
    (should (= (length args) 1))
    (should (plist-member (car args) :cursor))))

(ert-deftest badjuju-keymap-test/squash-file-calls-badjuju-squash ()
  (badjuju-test--with-captured-run
    (with-temp-buffer
      (setq buffer-file-name "/tmp/x.jujutsu")
      (badjuju-status-mode)
      (badjuju--run-squash-file)))
  (should (equal (caar badjuju-test-calls) "badjuju.squash")))

(ert-deftest badjuju-keymap-test/squash-file-requires-parent-prompts ()
  "When the server returns RequiresParentSelection, the parent prompt fires."
  (let (prompted)
    (cl-letf (((symbol-function 'badjuju-commands-run-with-handler)
               (lambda (_cmd _args on-error)
                 (funcall on-error
                          (list :code -32001
                                :message "needs parent"
                                :data (list :code "RequiresParentSelection"
                                            :file "src/x.rs"
                                            :candidates
                                            (vector
                                             (list :label "a" :id "id-a")
                                             (list :label "b" :id "id-b")))))))
              ((symbol-function 'badjuju-squash-with-parent-prompt)
               (lambda (file cands) (setq prompted (list file (length cands))))))
      (with-temp-buffer
        (setq buffer-file-name "/tmp/x.jujutsu")
        (badjuju-status-mode)
        (badjuju--run-squash-file))
      (should (equal prompted '("src/x.rs" 2))))))

;;; Rebase chord bindings

(ert-deftest badjuju-keymap-test/status-rebase-chords-bound ()
  "All six rebase chord keys must be bound in the status map."
  (dolist (key '("r s" "r r" "r b" "r o" "r A" "r B"))
    (should (functionp (lookup-key badjuju-status-mode-map (kbd key))))))

(ert-deftest badjuju-keymap-test/status-x-is-cancel ()
  (should (eq (lookup-key badjuju-status-mode-map (kbd "x"))
              #'badjuju--run-cancel)))

(ert-deftest badjuju-keymap-test/log-rebase-chords-bound ()
  "All six rebase chord keys must be bound in the log map."
  (dolist (key '("r s" "r r" "r b" "r o" "r A" "r B"))
    (should (functionp (lookup-key badjuju-log-mode-map (kbd key))))))

(ert-deftest badjuju-keymap-test/log-x-is-cancel ()
  (should (eq (lookup-key badjuju-log-mode-map (kbd "x"))
              #'badjuju--run-cancel)))

(ert-deftest badjuju-keymap-test/rebase-source-modes ()
  "Each rebase source mode lands in badjuju.rebase.source with the right mode."
  (dolist (mode '("source" "revisions" "branch"))
    (let ((badjuju-test-calls nil))
      (badjuju-test--with-captured-run
        (with-temp-buffer
          (setq buffer-file-name "/tmp/x.jujutsu")
          (badjuju-status-mode)
          (badjuju--run-rebase-source mode)))
      (should (equal (caar badjuju-test-calls) "badjuju.rebase.source"))
      (let ((args (cadar badjuju-test-calls)))
        (should (= (length args) 2))
        (should (equal (car args) mode))
        (should (plist-member (cadr args) :cursor))))))

(ert-deftest badjuju-keymap-test/rebase-commit-inserts ()
  "Each rebase commit insert mode lands in badjuju.rebase.commit with the right insert."
  (dolist (insert '("onto" "after" "before"))
    (let ((badjuju-test-calls nil))
      (badjuju-test--with-captured-run
        (with-temp-buffer
          (setq buffer-file-name "/tmp/x.jujutsu")
          (badjuju-status-mode)
          (badjuju--run-rebase-commit insert)))
      (should (equal (caar badjuju-test-calls) "badjuju.rebase.commit"))
      (let ((args (cadar badjuju-test-calls)))
        (should (= (length args) 2))
        (should (equal (car args) insert))
        (should (plist-member (cadr args) :cursor))))))

;;; fold-toggle dispatch

(require 'outline)

(ert-deftest badjuju-keymap-test/fold-toggle-uses-outline-toggle-children ()
  (let ((called nil))
    (with-temp-buffer
      ;; outline-minor-mode is buffer-local; setting it directly is correct here.
      (setq-local outline-minor-mode t)
      (cl-letf (((symbol-function 'outline-toggle-children)
                 (lambda () (setq called 'toggle-children))))
        (badjuju-keymap--fold-toggle)))
    (should (eq called 'toggle-children))))

(ert-deftest badjuju-keymap-test/fold-toggle-messages-when-no-support ()
  "Without outline support of any kind, surface a friendly message."
  (let (msg)
    (cl-letf (((symbol-function 'outline-toggle-children) nil)
              ((symbol-function 'outline-cycle) nil)
              ((symbol-function 'fboundp)
               (lambda (sym) (not (memq sym '(outline-toggle-children
                                              outline-cycle)))))
              ((symbol-function 'message)
               (lambda (fmt &rest args) (setq msg (apply #'format fmt args)))))
      (let ((outline-minor-mode nil))
        (badjuju-keymap--fold-toggle))
      (should (string-match-p "no fold support" msg)))))

(provide 'badjuju-keymap-test)
;;; badjuju-keymap-test.el ends here
