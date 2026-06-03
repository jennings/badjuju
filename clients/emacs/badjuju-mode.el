;;; badjuju-mode.el --- Major modes for Bad Juju buffers  -*- lexical-binding: t; -*-

;;; Commentary:

;; Defines `badjuju-mode', the parent major mode from which all Bad Juju
;; buffer-specific modes derive (status, log, diff, describe, squash).
;; Also registers the `jujutsu' filetype for *.jujutsu files.

;;; Code:

(defgroup badjuju nil
  "Emacs frontend for the Jujutsu VCS."
  :group 'tools
  :prefix "badjuju-")

(defvar badjuju-mode-map
  (let ((map (make-sparse-keymap)))
    map)
  "Keymap for `badjuju-mode' and all derived modes.")

(define-derived-mode badjuju-mode special-mode "BadJuju"
  "Parent major mode for Bad Juju buffers.
All buffer-specific modes (badjuju-status-mode, badjuju-log-mode,
badjuju-diff-mode, badjuju-describe-mode, badjuju-squash-mode) derive
from this mode.")

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.jujutsu\\'" . badjuju-mode))

(provide 'badjuju-mode)
;;; badjuju-mode.el ends here
