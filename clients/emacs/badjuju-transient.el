;;; badjuju-transient.el --- Transient menus for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Magit-style transient popup menus for Bad Juju.
;;
;;   `badjuju-commit' — Commit transient, bound to `c' in status and log
;;     buffers.  Starts small (reword, new); new actions can be added as
;;     server capability grows.
;;
;; The `?' help popup lives in `badjuju-keymap.el' as a regular side-window
;; because it is dynamically populated from the server's `badjuju.help' RPC.

;;; Code:

(require 'transient)
(require 'badjuju-commands)

;;; Commit transient (#45)

(transient-define-prefix badjuju-commit ()
  "Commit operations for Jujutsu.
Bound to `c' in status and log buffers."
  ["Create / amend"
   ("w" "Reword commit at cursor" badjuju-describe)
   ("n" "New child commit"        badjuju-new)])

(provide 'badjuju-transient)
;;; badjuju-transient.el ends here
