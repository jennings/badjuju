;;; badjuju-eglot.el --- Eglot server registration for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Registers the badjuju LSP server with Eglot for all badjuju major modes.
;; Sends initializationOptions matching the Neovim/VS Code clients:
;;   keymapProfile  — "magit" (default) or "none"
;;   virtualDiffs   — t (use workspace/textDocumentContent instead of disk files)
;;   binaryPath     — optional path to the badjuju binary
;;
;; Also hooks project.el so that .jj/ directories are recognized as project
;; roots, which is how Eglot determines the LSP workspace root.

;;; Code:

(require 'eglot)
(require 'project)
(require 'badjuju-mode)

;;; Customization

(defcustom badjuju-binary-path ""
  "Path to the badjuju binary.
When empty (the default), the binary is located on PATH."
  :type 'string
  :group 'badjuju)

(defcustom badjuju-keymap-profile "magit"
  "Keymap profile to request from the server.
\"magit\" (default) — Magit-style single-letter bindings.
\"none\" — no default bindings; configure your own."
  :type '(choice (const "magit") (const "none"))
  :group 'badjuju)

;;; Project backend — teaches project.el to treat .jj/ as a project root

(cl-defmethod project-root ((project (head badjuju)))
  "Return the root directory of a badjuju PROJECT."
  (cdr project))

(defun badjuju--project-find (dir)
  "Return a badjuju project cons if DIR is inside a jj workspace, else nil."
  (when-let ((root (locate-dominating-file dir ".jj")))
    (cons 'badjuju (expand-file-name root))))

(add-hook 'project-find-functions #'badjuju--project-find)

;;; LSP server class

(defclass badjuju-lsp-server (eglot-lsp-server) ()
  :documentation "Eglot server class for the Bad Juju LSP (badjuju lsp).")

(cl-defmethod eglot-initialization-options ((_server badjuju-lsp-server))
  "Build initializationOptions for the badjuju LSP server."
  (let ((opts (list :keymapProfile (if (and badjuju-keymap-profile
                                            (not (string= badjuju-keymap-profile "")))
                                       badjuju-keymap-profile
                                     "magit")
                    :virtualDiffs t)))
    (when (and badjuju-binary-path (not (string= badjuju-binary-path "")))
      (setq opts (append opts (list :binaryPath badjuju-binary-path))))
    opts))

;;; Server registration

(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
    (list '(badjuju-status-mode
            badjuju-log-mode
            badjuju-diff-mode
            badjuju-squash-mode
            badjuju-hunk-edit-mode
            badjuju-describe-mode)
          'badjuju-lsp-server "badjuju" "lsp")))

;;; Workspace root lookup

(defun badjuju--find-workspace-root (&optional dir)
  "Return the jj workspace root above DIR (or `default-directory'), or nil."
  (when-let ((root (locate-dominating-file (or dir default-directory) ".jj")))
    (expand-file-name root)))

;;; Server discovery / startup

(defvar badjuju--anchor-buffers (make-hash-table :test 'equal)
  "Maps workspace root strings to their anchor buffers for server connections.")

(defun badjuju--server-in-buffers (root)
  "Scan all buffers for a live badjuju Eglot server rooted at ROOT, or nil."
  (cl-loop for buf in (buffer-list)
           for srv = (with-current-buffer buf
                       (when (and (derived-mode-p 'badjuju-mode)
                                  (eglot-managed-p)
                                  (string-prefix-p
                                   root (expand-file-name default-directory)))
                         (eglot-current-server)))
           when srv return srv))

(defun badjuju--ensure-server ()
  "Return the badjuju Eglot server for the current workspace.
Starts one if none is running.  Signals an error when not inside a jj repo."
  (let ((root (badjuju--find-workspace-root)))
    (unless root
      (user-error "Not inside a jj workspace (no .jj/ directory found)"))
    (or
     ;; Current buffer is already managed
     (when (and (derived-mode-p 'badjuju-mode) (eglot-current-server))
       (eglot-current-server))
     ;; Another open buffer in the same workspace
     (badjuju--server-in-buffers root)
     ;; Start fresh via an anchor buffer
     (let* ((anchor-name (format " *badjuju:<%s>*"
                                 (file-name-nondirectory
                                  (directory-file-name root))))
            (anchor (or (gethash root badjuju--anchor-buffers)
                        (let ((b (generate-new-buffer anchor-name)))
                          (puthash root b badjuju--anchor-buffers)
                          b))))
       (with-current-buffer anchor
         (unless (derived-mode-p 'badjuju-mode)
           (setq default-directory root)
           ;; `eglot--guess-contact' uses major-mode only when buffer-file-name
           ;; is non-nil, and looks up the project via `project-find-functions'
           ;; relative to default-directory.  A sentinel path under .jj/badjuju/
           ;; satisfies both without colliding with any real file.
           (setq buffer-file-name (expand-file-name ".jj/badjuju/.anchor" root))
           (badjuju-status-mode))
         ;; Call `eglot--connect' directly: `eglot-ensure' schedules connection
         ;; via post-command-hook, which never fires during this synchronous call.
         (unless (eglot-current-server)
           (apply #'eglot--connect (eglot--guess-contact)))
         (eglot-current-server))))))

(provide 'badjuju-eglot)
;;; badjuju-eglot.el ends here
