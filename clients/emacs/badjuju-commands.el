;;; badjuju-commands.el --- LSP command wrappers for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Thin wrappers around workspace/executeCommand LSP requests.
;;
;; The key entry point is `badjuju-commands-run', which:
;;   1. Ensures the badjuju LSP server is running (starting it if needed).
;;   2. Sends workspace/executeCommand with the given command and arguments.
;;   3. Opens the returned file:// or badjuju-diff:// URI in the current window.
;;
;; Cursor-form arguments: many server commands accept a
;; (:cursor (:uri URI :line LINE)) argument that lets the server resolve the
;; revision or file under point.  Use `badjuju-commands-cursor-arg' to build one.

;;; Code:

(require 'badjuju-eglot)

;;; Cursor argument

(defun badjuju-commands-cursor-arg ()
  "Build a cursor-form argument for the current buffer position.
Returns a plist (:cursor (:uri URI :line LINE)) where LINE is 0-indexed.
This is the form the server uses to resolve the revision or file under point."
  (let* ((file (buffer-file-name))
         (uri (when file (concat "file://" (url-encode-url file))))
         (line (1- (line-number-at-pos))))
    (list :cursor (list :uri (or uri "") :line line))))

;;; Core execute helper

(defun badjuju-commands-run (command &optional args &rest _)
  "Send workspace/executeCommand COMMAND with ARGS to the badjuju LSP server.
ARGS is a list of arguments passed as the JSON array.  When the server
returns a URI string, open it in the current window."
  (let* ((server (badjuju--ensure-server))
         (result (jsonrpc-request
                  server
                  'workspace/executeCommand
                  (list :command command
                        :arguments (or (vconcat args) [])))))
    (when (and result (stringp result) (not (string= result "")))
      (badjuju-commands--open-uri result))))

(defun badjuju-commands--open-uri (uri)
  "Open a server-returned URI in the current window.
Handles file://, badjuju-diff://, and badjuju-file:// URIs."
  (cond
   ((string-prefix-p "badjuju-diff://" uri)
    (find-file uri))
   ((string-prefix-p "badjuju-file://" uri)
    (find-file uri))
   ((string-prefix-p "file://" uri)
    (find-file (url-filename (url-generic-parse-url uri))))
   (t
    (message "badjuju: unexpected URI %s" uri))))

(provide 'badjuju-commands)
;;; badjuju-commands.el ends here
