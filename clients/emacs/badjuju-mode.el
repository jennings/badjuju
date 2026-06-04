;;; badjuju-mode.el --- Major modes for Bad Juju buffers  -*- lexical-binding: t; -*-

;;; Commentary:

;; Defines the Bad Juju major mode hierarchy:
;;
;;   badjuju-mode (parent, derived from special-mode)
;;   ├── badjuju-status-mode    (read-only)
;;   ├── badjuju-log-mode       (writable — REVSET header triggers re-query on save)
;;   ├── badjuju-diff-mode      (read-only)
;;   ├── badjuju-squash-mode    (read-only)
;;   ├── badjuju-hunk-edit-mode (writable)
;;   └── badjuju-describe-mode  (writable)
;;
;; Also:
;;   - auto-mode-alist entries for *.jujutsu file patterns (#35)
;;   - workspace/textDocumentContent/refresh notification handler (#42)
;;   - auto-revert-mode for file-URI jujutsu buffers (#42)
;;   - describe-buffer C-c C-c / C-c C-k keybindings (#43)

;;; Code:

(require 'eglot)
(require 'autorevert)

(defgroup badjuju nil
  "Emacs frontend for the Jujutsu VCS."
  :group 'tools
  :prefix "badjuju-")

;;; Parent mode

(defvar badjuju-mode-map
  (let ((map (make-sparse-keymap)))
    map)
  "Keymap shared by all Bad Juju buffer modes.")

(define-derived-mode badjuju-mode special-mode "BadJuju"
  "Parent major mode for all Bad Juju buffers.
All buffer-specific modes derive from this one."
  (setq-local comment-start "JJ: ")
  (setq-local comment-end "")
  ;; Enable auto-revert so on-disk rewrites (status/log/diff in file-mode
  ;; clients, and Eglot workspace/applyEdit) are picked up without manual M-x
  ;; revert-buffer.
  (auto-revert-mode 1))

;;; Per-buffer derived modes

(define-derived-mode badjuju-status-mode badjuju-mode "BadJuju/Status"
  "Major mode for Bad Juju status buffers (status.jujutsu).
The buffer is read-only; the server regenerates it after every mutation."
  (setq buffer-read-only t))

(define-derived-mode badjuju-log-mode badjuju-mode "BadJuju/Log"
  "Major mode for Bad Juju log buffers (log.jujutsu).
The REVSET: header line is editable; saving re-runs the log query."
  (setq buffer-read-only nil))

(define-derived-mode badjuju-diff-mode badjuju-mode "BadJuju/Diff"
  "Major mode for Bad Juju diff buffers (diff*.jujutsu).
The buffer is read-only."
  (setq buffer-read-only t))

(define-derived-mode badjuju-squash-mode badjuju-mode "BadJuju/Squash"
  "Major mode for Bad Juju squash buffers (squash/*.jujutsu).
The buffer is read-only; individual hunks are toggled via keybindings."
  (setq buffer-read-only t))

(define-derived-mode badjuju-hunk-edit-mode badjuju-mode "BadJuju/HunkEdit"
  "Major mode for Bad Juju hunk-edit buffers (hunk-edit.jujutsu).
The buffer is writable; saving commits the edited hunk."
  (setq buffer-read-only nil))

(defvar badjuju-describe-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-c") #'badjuju-describe-finish)
    (define-key map (kbd "C-c C-k") #'badjuju-describe-abort)
    map)
  "Keymap for `badjuju-describe-mode'.")

(define-derived-mode badjuju-describe-mode badjuju-mode "BadJuju/Describe"
  "Major mode for Bad Juju describe buffers (describe.jujutsu).
\\<badjuju-describe-mode-map>
\\[badjuju-describe-finish] saves the buffer and triggers textDocument/didSave so
the server applies the new description.
\\[badjuju-describe-abort] reverts and closes without applying."
  (setq buffer-read-only nil))

(defun badjuju-describe-finish ()
  "Save describe.jujutsu and bury the buffer.
Saving triggers textDocument/didSave; the server applies the description and
regenerates status/log buffers."
  (interactive)
  (save-buffer)
  (bury-buffer))

(defun badjuju-describe-abort ()
  "Revert describe.jujutsu and bury the buffer without applying the description."
  (interactive)
  (set-buffer-modified-p nil)
  (bury-buffer))

;;; workspace/textDocumentContent/refresh handler (#42)
;;
;; The server sends this custom notification after any mutating command that
;; changes the content of a virtual-URI buffer.  We re-fetch the content from
;; the server and replace the buffer contents, preserving point where possible.

(defun badjuju--refresh-virtual-buffer (uri)
  "Re-fetch content for the virtual buffer identified by URI and replace it."
  (dolist (buf (buffer-list))
    (with-current-buffer buf
      (when (and (derived-mode-p 'badjuju-mode)
                 (equal (buffer-name) uri))
        (when-let ((server (eglot-current-server)))
          (condition-case err
              (let* ((result (jsonrpc-request
                              server
                              'workspace/textDocumentContent
                              (list :uri uri)))
                     (text (plist-get result :text))
                     (inhibit-read-only t)
                     (saved-point (point)))
                (when (stringp text)
                  (erase-buffer)
                  (insert text)
                  (goto-char (min saved-point (point-max)))))
            (error
             (message "badjuju: failed to refresh %s: %s" uri (error-message-string err)))))))))

(cl-defmethod eglot-handle-notification
  (_server (_method (eql workspace/textDocumentContent/refresh))
           &key uri &allow-other-keys)
  "Handle badjuju's custom `workspace/textDocumentContent/refresh' notification.
Re-fetches the virtual buffer identified by URI from the server."
  (when uri
    (badjuju--refresh-virtual-buffer uri)))

;;; Filetype detection
;;
;; More-specific patterns must precede the catch-all \.jujutsu\' entry so
;; auto-mode-alist stops at the right mode.  add-to-list prepends, so we add
;; from least-specific to most-specific.

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.jujutsu\\'" . badjuju-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/describe\\.jujutsu\\'" . badjuju-describe-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/hunk-edit\\.jujutsu\\'" . badjuju-hunk-edit-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/squash/[^/]+\\.jujutsu\\'" . badjuju-squash-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/diff[^/]*\\.jujutsu\\'" . badjuju-diff-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/log\\.jujutsu\\'" . badjuju-log-mode))
;;;###autoload
(add-to-list 'auto-mode-alist '("/\\.jj/badjuju/status\\.jujutsu\\'" . badjuju-status-mode))

(provide 'badjuju-mode)
;;; badjuju-mode.el ends here
