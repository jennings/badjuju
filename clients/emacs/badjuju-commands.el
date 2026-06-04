;;; badjuju-commands.el --- LSP command wrappers for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Thin wrappers around workspace/executeCommand LSP requests.
;;
;; Key entry points:
;;   `badjuju-commands-run'      — send a command, open the returned URI
;;   `badjuju-commands-request'  — send a command, pass raw result to a callback
;;   `badjuju-commands-cursor-arg' — build a cursor-form {:cursor {:uri :line}} plist

;;; Code:

(require 'url-parse)
(require 'badjuju-eglot)

;;; Cursor argument

(defun badjuju-commands-cursor-arg ()
  "Build a cursor-form argument for the current buffer position.
Returns a plist (:cursor (:uri URI :line LINE)) where LINE is 0-indexed."
  (let* ((file (buffer-file-name))
         (uri (when file (concat "file://" (url-encode-url file))))
         (line (1- (line-number-at-pos))))
    (list :cursor (list :uri (or uri "") :line line))))

;;; Core execute helpers

(defun badjuju-commands-run (command &optional args)
  "Send workspace/executeCommand COMMAND with ARGS to the badjuju LSP server.
ARGS is a list of arguments.  When the server returns a URI string, open it.
Signals a user-error on LSP error; callers that need custom error handling
should use `badjuju-commands-run-with-handler' instead."
  (let* ((server (badjuju--ensure-server))
         (result (jsonrpc-request
                  server
                  'workspace/executeCommand
                  (list :command command
                        :arguments (or (vconcat args) [])))))
    (when (and result (stringp result) (not (string= result "")))
      (badjuju-commands--open-uri result))))

(defun badjuju-commands-run-with-handler (command args on-error)
  "Like `badjuju-commands-run' but call ON-ERROR instead of signaling on failure.
ON-ERROR receives the jsonrpc error plist (:code :message :data)."
  (let ((server (badjuju--ensure-server)))
    (condition-case err
        (let ((result (jsonrpc-request
                       server
                       'workspace/executeCommand
                       (list :command command
                             :arguments (or (vconcat args) [])))))
          (when (and result (stringp result) (not (string= result "")))
            (badjuju-commands--open-uri result)))
      (jsonrpc-error
       (when on-error
         ;; err = (jsonrpc-error "msg" (:jsonrpc-error-code C :jsonrpc-error-message M :jsonrpc-error-data D))
         (let* ((details (cddr err))
                (code    (plist-get details :jsonrpc-error-code))
                (msg     (plist-get details :jsonrpc-error-message))
                (data    (plist-get details :jsonrpc-error-data)))
           (funcall on-error (list :code code :message msg :data data))))))))

(defun badjuju-commands-request (command args callback)
  "Send workspace/executeCommand COMMAND with ARGS and pass raw result to CALLBACK."
  (let* ((server (badjuju--ensure-server))
         (result (jsonrpc-request
                  server
                  'workspace/executeCommand
                  (list :command command
                        :arguments (or (vconcat args) [])))))
    (funcall callback result)))

(defun badjuju-commands--open-uri (uri)
  "Open a server-returned URI in the current window."
  (cond
   ((string-prefix-p "badjuju-diff://" uri) (find-file uri))
   ((string-prefix-p "badjuju-file://" uri) (find-file uri))
   ((string-prefix-p "file://"         uri)
    (find-file (url-filename (url-generic-parse-url uri))))
   (t
    (message "badjuju: unexpected URI %s" uri))))

(provide 'badjuju-commands)
;;; badjuju-commands.el ends here
