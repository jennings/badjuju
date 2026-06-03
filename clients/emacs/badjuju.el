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
;;   M-x badjuju-diff      open a diff for the current change

;;; Code:

(require 'badjuju-eglot)
(require 'badjuju-mode)
(require 'badjuju-commands)
(require 'badjuju-keymap)
(require 'badjuju-uri)
(require 'badjuju-transient)
(require 'badjuju-prompts)

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
(defun badjuju-describe (&optional revision)
  "Open the describe buffer for REVISION (default: current change)."
  (interactive)
  (badjuju-commands-run "badjuju.describe" (when revision (list revision))))

;;;###autoload
(defun badjuju-diff (&optional revision)
  "Open a diff for REVISION (default: current change)."
  (interactive)
  (badjuju-commands-run "badjuju.diff" (when revision (list revision))))

;;;###autoload
(defun badjuju-diff-commit (&optional revision)
  "Open a pinned commit-id diff for REVISION (default: current change)."
  (interactive)
  (badjuju-commands-run "badjuju.diff.commit" (when revision (list revision))))

;;;###autoload
(defun badjuju-new ()
  "Create a new change as a child of the change at point (or @)."
  (interactive)
  (badjuju-commands-run "badjuju.new"))

;;;###autoload
(defun badjuju-squash ()
  "Squash the current change into its parent."
  (interactive)
  (badjuju-commands-run "badjuju.squash"))

;;;###autoload
(defun badjuju-unsquash ()
  "Move content from the parent change back into the child."
  (interactive)
  (badjuju-commands-run "badjuju.unsquash"))

;;;###autoload
(defun badjuju-undo ()
  "Undo the last jj operation."
  (interactive)
  (badjuju-commands-run "badjuju.undo"))

;;;###autoload
(defun badjuju-abandon (&optional revision)
  "Abandon REVISION (default: @)."
  (interactive)
  (badjuju-commands-run "badjuju.abandon" (list (or revision "@"))))

;;;###autoload
(defun badjuju-edit (&optional revision)
  "Edit REVISION (default: @)."
  (interactive)
  (badjuju-commands-run "badjuju.edit" (list (or revision "@"))))

;;;###autoload
(defun badjuju-fetch ()
  "Fetch from all remotes."
  (interactive)
  (badjuju-commands-run "badjuju.fetch"))

;;;###autoload
(defun badjuju-push (&optional force)
  "Push bookmarks.  With prefix arg FORCE, push with --force-with-lease."
  (interactive "P")
  (badjuju-commands-run "badjuju.push" (list (list :forceWithLease (if force t :json-false)))))

(provide 'badjuju)
;;; badjuju.el ends here
