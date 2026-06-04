;;; badjuju.el --- Jujutsu VCS frontend for Emacs  -*- lexical-binding: t; -*-

;; Package-Requires: ((emacs "29.1"))
;; Version: 0.1.0
;; Keywords: vc tools
;; URL: https://github.com/jennings/badjuju

;;; Commentary:

;; Bad Juju is an LSP-powered frontend for the Jujutsu VCS (jj),
;; modeled on Magit.  Requires Emacs 29+ for built-in eglot and
;; transient.
;;
;; Quick start:
;;   M-x badjuju-status    open the status buffer
;;   M-x badjuju-log       open the log buffer
;;   M-x badjuju-diff      open a diff for the change at point

;;; Code:

(require 'badjuju-eglot)
(require 'badjuju-mode)
(require 'badjuju-commands)
(require 'badjuju-keymap)
(require 'badjuju-uri)
(require 'badjuju-transient)
(require 'badjuju-prompts)

;;; No-argument commands

;;;###autoload
(defun badjuju-status ()
  "Open the Bad Juju status buffer."
  (interactive)
  (badjuju-commands-run "badjuju.status"))

;;;###autoload
(defun badjuju-log (&optional revset)
  "Open the Bad Juju log buffer, optionally filtered to REVSET."
  (interactive)
  (badjuju-commands-run "badjuju.log" (when revset (list revset))))

;;;###autoload
(defun badjuju-undo ()
  "Undo the last jj operation."
  (interactive)
  (badjuju-commands-run "badjuju.undo"))

;;;###autoload
(defun badjuju-fetch ()
  "Fetch from all remotes."
  (interactive)
  (badjuju-commands-run "badjuju.fetch"))

;;;###autoload
(defun badjuju-push (&optional force)
  "Push bookmarks.  With prefix arg FORCE, push with --force-with-lease."
  (interactive "P")
  (badjuju-commands-run "badjuju.push"
                        (list (list :forceWithLease (if force t :json-false)))))

;;; Cursor-aware commands
;;
;; When invoked from a badjuju status/log/diff buffer, these ship the cursor
;; position so the server can resolve the revision or file under point.

;;;###autoload
(defun badjuju-new ()
  "Create a new change as a child of the change at point (or @)."
  (interactive)
  (if (derived-mode-p 'badjuju-mode)
      (badjuju-commands-run "badjuju.new" (list (badjuju-commands-cursor-arg)))
    (badjuju-commands-run "badjuju.new")))

;;;###autoload
(defun badjuju-describe ()
  "Open the describe buffer for the change at point (or @)."
  (interactive)
  (badjuju-commands-run "badjuju.describe"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-diff ()
  "Open a change-mode diff for the revision at point (or @).
The diff updates on every subsequent amend."
  (interactive)
  (badjuju-commands-run "badjuju.diff"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-diff-commit ()
  "Open a commit-mode diff for the revision at point (or @).
The diff is pinned to the exact commit and never refreshed."
  (interactive)
  (badjuju-commands-run "badjuju.diff.commit"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-edit ()
  "Move @ to the revision at point."
  (interactive)
  (badjuju-commands-run "badjuju.edit"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-abandon ()
  "Abandon the revision at point (or @ if not in a badjuju buffer)."
  (interactive)
  (badjuju-commands-run "badjuju.abandon"
                        (if (derived-mode-p 'badjuju-mode)
                            (list (badjuju-commands-cursor-arg))
                          (list "@"))))

;;;###autoload
(defun badjuju-squash ()
  "Begin or complete a commit-to-commit squash at point."
  (interactive)
  (badjuju-commands-run "badjuju.squash.commit"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-squash-file ()
  "Squash the file at point from the current change into its parent."
  (interactive)
  (badjuju-commands-run "badjuju.squash"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-unsquash ()
  "Unsquash the file at point from parent into the child change."
  (interactive)
  (badjuju-commands-run "badjuju.unsquash"
                        (when (derived-mode-p 'badjuju-mode)
                          (list (badjuju-commands-cursor-arg)))))

;;;###autoload
(defun badjuju-refresh ()
  "Refresh the current Bad Juju buffer."
  (interactive)
  (let ((uri (when (buffer-file-name)
               (concat "file://" (buffer-file-name)))))
    (badjuju-commands-run "badjuju.refresh" (list (or uri "")))))

(provide 'badjuju)
;;; badjuju.el ends here
