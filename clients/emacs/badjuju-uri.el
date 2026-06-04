;;; badjuju-uri.el --- URI scheme handlers for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Handlers for the `badjuju-diff://' and `badjuju-file://' virtual URI
;; schemes emitted by the server.  Installs entries in
;; `file-name-handler-alist' so that `find-file' (and the commands in
;; badjuju.el that call it) can open virtual URIs directly.
;;
;; badjuju-diff://  — server-rendered unified diff for a change or commit
;; badjuju-file://  — a file blob at a specific commit, read-only

;;; Code:

(require 'eglot)
(require 'badjuju-mode)
(require 'badjuju-eglot)

;;; File-name handler

(defconst badjuju-uri--handler-regexp
  "\\`badjuju-\\(diff\\|file\\)://"
  "Regexp matching the URI schemes handled by `badjuju-uri--handler'.")

(defun badjuju-uri--handler (operation &rest args)
  "File-name handler for badjuju-diff:// and badjuju-file:// URIs.
Dispatches OPERATION; only `insert-file-contents' is implemented — all
other operations fall through to the default handler."
  (if (eq operation 'insert-file-contents)
      (apply #'badjuju-uri--insert-file-contents args)
    (let ((inhibit-file-name-handlers
           (cons #'badjuju-uri--handler inhibit-file-name-handlers))
          (inhibit-file-name-operation operation))
      (apply operation args))))

(defun badjuju-uri--insert-file-contents (uri &optional _visit _beg _end _replace)
  "Fetch content for URI via workspace/textDocumentContent and insert it."
  (let ((server (badjuju--ensure-server)))
    (unless server
      (error "badjuju: no LSP server available for %s" uri))
    (condition-case err
        (let* ((result (jsonrpc-request server
                                        'workspace/textDocumentContent
                                        (list :uri uri)))
               (text (or (plist-get result :text) "")))
          ;; Set buffer name to the URI so the refresh handler can find it.
          (setq buffer-file-name uri)
          (insert text)
          ;; Set the appropriate major mode.
          (cond
           ((string-prefix-p "badjuju-diff://" uri) (badjuju-diff-mode))
           ((string-prefix-p "badjuju-file://" uri)
            ;; Infer filetype from the path component after the commit ID.
            ;; badjuju-file:///commit/<id>/<repo-relative/path.rs>
            (let ((path (or (and (string-match
                                  "badjuju-file://+commit/[^/]+/\\(.*\\)$" uri)
                                 (match-string 1 uri))
                            "")))
              (let ((mode (assoc-default path auto-mode-alist #'string-match)))
                (when (functionp mode) (funcall mode))))))
          (setq buffer-read-only t)
          (list uri (length text)))
      (error
       (error "badjuju: failed to fetch %s: %s" uri (error-message-string err))))))

;;;###autoload
(defun badjuju-uri-setup ()
  "Register badjuju URI scheme handlers in `file-name-handler-alist'."
  (add-to-list 'file-name-handler-alist
               (cons badjuju-uri--handler-regexp #'badjuju-uri--handler)))

;; Register automatically when this file is loaded.
(badjuju-uri-setup)

(provide 'badjuju-uri)
;;; badjuju-uri.el ends here
