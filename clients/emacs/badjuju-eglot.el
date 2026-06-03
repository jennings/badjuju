;;; badjuju-eglot.el --- Eglot server registration for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Registers the Bad Juju LSP server with Eglot, sets initializationOptions
;; (binary path, virtual diffs, commandReference overrides), and provides
;; helpers for obtaining the active server connection.
;; Full implementation lands in issue #37.

;;; Code:

(provide 'badjuju-eglot)
;;; badjuju-eglot.el ends here
