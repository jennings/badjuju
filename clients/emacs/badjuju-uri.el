;;; badjuju-uri.el --- URI scheme handlers for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Handlers for the `badjuju-diff://' and `badjuju-file://' URI schemes.
;; These virtual URIs are served via workspace/textDocumentContent and
;; let the server deliver diff and source-file content without writing
;; files to disk.
;; Full implementation lands in issue #44.

;;; Code:

(provide 'badjuju-uri)
;;; badjuju-uri.el ends here
