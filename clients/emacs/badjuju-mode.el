;;; badjuju-mode.el --- Major modes for Bad Juju buffers  -*- lexical-binding: t; -*-

;;; Commentary:

;; Defines the Bad Juju major mode hierarchy:
;;
;;   badjuju-mode (parent, derived from special-mode)
;;   ├── badjuju-status-mode    (read-only)
;;   ├── badjuju-log-mode       (writable — REVSET header triggers re-query on save)
;;   ├── badjuju-diff-mode      (read-only)
;;   ├── badjuju-squash-mode    (read-only)
;;   ├── badjuju-hunk-edit-mode (writable)
;;   └── badjuju-describe-mode  (writable)
;;
;; Also registers auto-mode-alist entries for *.jujutsu file patterns.

;;; Code:

(defgroup badjuju nil
  "Emacs frontend for the Jujutsu VCS."
  :group 'tools
  :prefix "badjuju-")

;;; Parent mode

(defvar badjuju-mode-map
  (let ((map (make-sparse-keymap)))
    map)
  "Keymap shared by all Bad Juju buffer modes.")

(define-derived-mode badjuju-mode special-mode "BadJuju"
  "Parent major mode for all Bad Juju buffers.
All buffer-specific modes derive from this one."
  (setq-local comment-start "JJ: ")
  (setq-local comment-end ""))

;;; Per-buffer derived modes

(define-derived-mode badjuju-status-mode badjuju-mode "BadJuju/Status"
  "Major mode for Bad Juju status buffers (status.jujutsu).
The buffer is read-only; the server regenerates it after every mutation."
  (setq buffer-read-only t))

(define-derived-mode badjuju-log-mode badjuju-mode "BadJuju/Log"
  "Major mode for Bad Juju log buffers (log.jujutsu).
The REVSET: header line is editable; saving re-runs the log query."
  (setq buffer-read-only nil))

(define-derived-mode badjuju-diff-mode badjuju-mode "BadJuju/Diff"
  "Major mode for Bad Juju diff buffers (diff*.jujutsu).
The buffer is read-only."
  (setq buffer-read-only t))

(define-derived-mode badjuju-squash-mode badjuju-mode "BadJuju/Squash"
  "Major mode for Bad Juju squash buffers (squash/*.jujutsu).
The buffer is read-only; individual hunks are toggled via keybindings."
  (setq buffer-read-only t))

(define-derived-mode badjuju-hunk-edit-mode badjuju-mode "BadJuju/HunkEdit"
  "Major mode for Bad Juju hunk-edit buffers (hunk-edit.jujutsu).
The buffer is writable; saving commits the edited hunk."
  (setq buffer-read-only nil))

(define-derived-mode badjuju-describe-mode badjuju-mode "BadJuju/Describe"
  "Major mode for Bad Juju describe buffers (describe.jujutsu).
The buffer is writable; \\[badjuju-describe-finish] saves and applies the description."
  (setq buffer-read-only nil))

;;; Filetype detection
;;
;; More-specific patterns must precede the catch-all \.jujutsu\' entry so
;; auto-mode-alist stops at the right mode.  add-to-list prepends, so we add
;; from least-specific to most-specific.

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.jujutsu\\'" . badjuju-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/describe\\.jujutsu\\'" . badjuju-describe-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/hunk-edit\\.jujutsu\\'" . badjuju-hunk-edit-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/squash/[^/]+\\.jujutsu\\'" . badjuju-squash-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/diff[^/]*\\.jujutsu\\'" . badjuju-diff-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/log\\.jujutsu\\'" . badjuju-log-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/status\\.jujutsu\\'" . badjuju-status-mode))

(provide 'badjuju-mode)
;;; badjuju-mode.el ends here
