;;; badjuju-commands.el --- LSP command wrappers for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Thin wrappers around `workspace/executeCommand' LSP requests.
;; Full implementation lands in issue #36.

;;; Code:

(declare-function badjuju-eglot-server "badjuju-eglot")

(defun badjuju-commands-run (command &optional args)
  "Execute COMMAND via the Bad Juju LSP server with optional ARGS list."
  (error "Bad Juju: LSP wiring not yet set up (see badjuju-eglot.el)"))

(provide 'badjuju-commands)
;;; badjuju-commands.el ends here
