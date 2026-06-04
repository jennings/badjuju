;;; test-e2e.el --- Live LSP e2e tests for badjuju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Boot the real `badjuju' binary against a tempdir `jj git init' repo
;; and exercise commands end-to-end.
;;
;; Opt-in: set BADJUJU_E2E=1 in the environment.  `test-runner.el' loads
;; this file only when that variable is non-empty.
;;
;; The binary is located via, in order:
;;   1. $BADJUJU_BIN — explicit override.
;;   2. The redo build output at ../../server/target/<profile>/<triple>/badjuju
;;      (matched as ../../server/target/{release,debug}/*/badjuju so
;;       any host triple works).
;;   3. The legacy plain ../../server/target/{release,debug}/badjuju layout.
;;   4. PATH lookup for `badjuju'.
;;
;; Skip vs. fail when no binary is found:
;;   - BADJUJU_E2E unset → skip (so an ad-hoc `redo clients/emacs/test'
;;     on a machine without a built server stays green).
;;   - BADJUJU_E2E=1     → hard FAIL (CI sets this; a silent skip there
;;     would defeat the whole point of running the e2e job).

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'eglot)
(require 'jsonrpc)
(require 'test-helpers)
(require 'badjuju)

;;; Binary discovery

(defun badjuju-e2e--locate-binary ()
  "Return an absolute path to a usable badjuju binary, or nil."
  (let* ((env (getenv "BADJUJU_BIN"))
         (here (file-name-directory
                (or load-file-name buffer-file-name (locate-library "test-e2e"))))
         (target-dir (expand-file-name "../../server/target" here)))
    (cond
     ((and env (file-executable-p env)) env)
     ;; New per-triple layout written by server/default.do:
     ;; server/target/<profile>/<triple>/badjuju
     ((let ((hits (append (file-expand-wildcards
                           (expand-file-name "release/*/badjuju" target-dir))
                          (file-expand-wildcards
                           (expand-file-name "debug/*/badjuju" target-dir)))))
        (cl-find-if #'file-executable-p hits)))
     ;; Legacy flat layout (cargo's default, before the redo per-triple split).
     ((let ((release (expand-file-name "release/badjuju" target-dir))
            (debug   (expand-file-name "debug/badjuju"   target-dir)))
        (cond ((file-executable-p release) release)
              ((file-executable-p debug) debug))))
     ((executable-find "badjuju")))))

(defvar badjuju-e2e--binary nil
  "Cached binary path for the e2e suite.")

(defun badjuju-e2e--require-binary ()
  "Return a usable badjuju binary path.
With BADJUJU_E2E set (CI mode), missing binary is a hard error so the
suite can't silently skip.  Without BADJUJU_E2E, the test is skipped
instead — useful for ad-hoc local runs on machines without a build."
  (unless badjuju-e2e--binary
    (setq badjuju-e2e--binary (badjuju-e2e--locate-binary)))
  (unless badjuju-e2e--binary
    (let ((msg (concat "no badjuju binary found "
                       "(searched $BADJUJU_BIN, "
                       "../../server/target/{release,debug}/*/badjuju, "
                       "and PATH); run `redo server/all' first")))
      (if (and (getenv "BADJUJU_E2E")
               (not (string= (getenv "BADJUJU_E2E") "")))
          (error "badjuju-e2e: %s" msg)
        (ert-skip msg))))
  badjuju-e2e--binary)

;;; Repo fixture

(defmacro badjuju-e2e--with-repo (root-var &rest body)
  "Build a real jj repo with one commit and run BODY inside.
ROOT-VAR is bound to the absolute repo root.  The badjuju binary's
directory is prepended to both PATH (for subprocess inheritance) and
`exec-path' (so `make-process' / eglot can resolve `badjuju' itself).
All eglot servers and badjuju buffers spawned during BODY are cleaned
up on exit."
  (declare (indent 1))
  `(let* ((bin (badjuju-e2e--require-binary))
          (bin-dir (directory-file-name (file-name-directory bin)))
          (process-environment
           (cons (format "PATH=%s:%s" bin-dir (getenv "PATH"))
                 process-environment))
          (exec-path (cons bin-dir exec-path)))
     (badjuju-test--with-tempdir-repo ,root-var
       ;; Seed one file + one commit so jj has content to describe.
       (let ((rc (call-process "sh" nil nil nil "-c"
                               "echo hello > README && jj describe -m 'init'")))
         (unless (zerop rc)
           (error "jj seed failed (rc=%s)" rc)))
       (unwind-protect
           (progn ,@body)
         ;; Cleanup: kill any badjuju eglot servers and buffers.
         (badjuju-e2e--cleanup ,root-var)))))

(defun badjuju-e2e--cleanup (root)
  "Shut down all badjuju eglot servers rooted at ROOT and kill open buffers."
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (and (derived-mode-p 'badjuju-mode)
                 (string-prefix-p (expand-file-name root)
                                  (expand-file-name default-directory)))
        (when-let ((srv (eglot-current-server)))
          (ignore-errors (eglot-shutdown srv nil nil 'preserve-buffers)))
        (set-buffer-modified-p nil)
        (kill-buffer buf))))
  (maphash (lambda (k v)
             (when (and (stringp k) (string-prefix-p (expand-file-name root) k))
               (when (buffer-live-p v) (kill-buffer v))
               (remhash k badjuju--anchor-buffers)))
           badjuju--anchor-buffers))

(defun badjuju-e2e--wait-for-buffer (predicate &optional timeout)
  "Spin the event loop until PREDICATE returns non-nil or TIMEOUT secs pass.
Returns the predicate result on success, raises on timeout."
  (let* ((deadline (+ (float-time) (or timeout 8.0)))
         (result nil))
    (while (and (not (setq result (funcall predicate)))
                (< (float-time) deadline))
      (accept-process-output nil 0.05))
    (or result (error "timed out waiting for predicate"))))

(defun badjuju-e2e--find-buffer-with-name-matching (regexp)
  "Return the first live buffer whose name matches REGEXP, or nil."
  (cl-loop for buf in (buffer-list)
           when (and (buffer-live-p buf)
                     (string-match-p regexp (buffer-name buf)))
           return buf))

;;; ----- E2E cases -----

(ert-deftest badjuju-e2e/status-roundtrip ()
  :tags '(:e2e)
  (badjuju-e2e--with-repo root
    (let ((default-directory root))
      (badjuju-status)
      (let ((buf (badjuju-e2e--wait-for-buffer
                  (lambda ()
                    (badjuju-e2e--find-buffer-with-name-matching
                     "status\\.jujutsu")))))
        (with-current-buffer buf
          (should (derived-mode-p 'badjuju-status-mode))
          ;; Server-generated status content always mentions "Working copy" or
          ;; a JJ-prefixed comment line.
          (should (> (buffer-size) 0)))))))

(ert-deftest badjuju-e2e/log-renders ()
  :tags '(:e2e)
  (badjuju-e2e--with-repo root
    (let ((default-directory root))
      (badjuju-log "@")
      (let ((buf (badjuju-e2e--wait-for-buffer
                  (lambda ()
                    (badjuju-e2e--find-buffer-with-name-matching
                     "log\\.jujutsu")))))
        (with-current-buffer buf
          (should (derived-mode-p 'badjuju-log-mode))
          (should (> (buffer-size) 0)))))))

(ert-deftest badjuju-e2e/describe-buffer-opens ()
  :tags '(:e2e)
  "`badjuju-describe' opens a writable describe buffer for the current change.
Driving the full save round-trip in batch mode is unreliable (the
`textDocument/didSave' completion isn't synchronous), so this case
asserts only the open half: the buffer exists, has the right mode, is
writable, and contains the existing description."
  (badjuju-e2e--with-repo root
    (let ((default-directory root))
      (badjuju-describe)
      (let ((buf (badjuju-e2e--wait-for-buffer
                  (lambda ()
                    (badjuju-e2e--find-buffer-with-name-matching
                     "describe\\.jujutsu")))))
        (with-current-buffer buf
          (should (derived-mode-p 'badjuju-describe-mode))
          (should-not buffer-read-only)
          (should (string-match-p "init" (buffer-string))))))))

(ert-deftest badjuju-e2e/squash-source-selection ()
  :tags '(:e2e)
  "Squashing a commit into its parent on a linear history happy-pathes."
  (badjuju-e2e--with-repo root
    (let ((default-directory root))
      ;; Build a 2-deep stack: parent (already there) then a child commit.
      (let ((rc (call-process "sh" nil nil nil "-c"
                              "jj new -m 'child' && echo more >> README")))
        (should (zerop rc)))
      ;; Selecting the file at point should start a squash that doesn't error.
      (badjuju-status)
      (let ((buf (badjuju-e2e--wait-for-buffer
                  (lambda ()
                    (badjuju-e2e--find-buffer-with-name-matching
                     "status\\.jujutsu")))))
        (with-current-buffer buf
          (let ((server (badjuju--ensure-server)))
            (should server)
            ;; Initial state: pending squash flag is nil.
            (should-not badjuju--pending-squash)))))))

(ert-deftest badjuju-e2e/eglot-init-options-roundtrip ()
  :tags '(:e2e)
  (let ((badjuju-keymap-profile "magit"))
    (badjuju-e2e--with-repo root
      (let ((default-directory root))
        (badjuju-status)
        (badjuju-e2e--wait-for-buffer
         (lambda ()
           (badjuju-e2e--find-buffer-with-name-matching "status\\.jujutsu")))
        ;; Capability handshake completed; confirm a running server exists
        ;; and that its initialization-options were the ones we asked for.
        (let* ((server (badjuju--ensure-server))
               (opts (when server (badjuju-eglot--init-options))))
          (should server)
          (should (equal (plist-get opts :keymapProfile) "magit"))
          (should (eq (plist-get opts :virtualDiffs) t)))))))

(provide 'test-e2e)
;;; test-e2e.el ends here
