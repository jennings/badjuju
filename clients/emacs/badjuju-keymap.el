;;; badjuju-keymap.el --- Magit-style keymaps for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Installs Magit-style buffer-local keymaps for all badjuju modes.
;; Called from mode hooks set up by `badjuju-keymap-setup'.
;;
;; Status & log buffers (#39):
;;   g / R   refresh           n   new
;;   d       diff (change)     D   diff (commit-pinned)
;;   s       squash source     S   squash file at cursor
;;   u       unsquash          U   undo
;;   a       abandon           e   edit (move @)
;;   r s/r/b rebase source     r o/A/B  rebase dest
;;   x       cancel pending    b   bookmark
;;   f       fetch             p   push        P   push --force
;;   L       log (status only) q   bury        TAB fold toggle
;;   ?       help popup
;;   c       commit transient  =   diff (same as d)
;;
;; Code actions intentionally do NOT have a default binding here — use
;; the editor's native binding (Emacs: `M-x eglot-code-actions`, or
;; whatever you've bound it to globally).
;;
;; Diff buffers (#39):
;;   g / R   refresh           q   bury
;;   ?       help popup
;;   gd      goto definition
;;
;; Squash buffers (#41):
;;   s   toggle hunk/file      a   select all     A   select none
;;   u   undo                  e   edit hunk      TAB fold toggle
;;   q   bury                  ?   help popup     gd  goto def
;;
;; Folding (#49):
;;   Uses LSP folding ranges from Eglot on status and squash buffers.
;;   Starts fully folded, then expands WORKING COPY CHANGES / PARENT CHANGES.

;;; Code:

(require 'badjuju-mode)
(require 'badjuju-commands)
(require 'badjuju-prompts)
(require 'badjuju-transient)

;;; Help popup

(defun badjuju--show-help (window-type)
  "Show a floating help popup for WINDOW-TYPE (\"status\", \"log\", etc.)."
  (let* ((server (badjuju--ensure-server))
         (result (condition-case err
                     (jsonrpc-request server 'workspace/executeCommand
                                      (list :command "badjuju.help"
                                            :arguments (vector window-type)))
                   (error
                    (message "badjuju: help failed: %s" (error-message-string err))
                    nil))))
    (when (and result (listp result) (> (length result) 0))
      (let* ((entries (append result nil))
             (max-key (cl-loop for e in entries
                               maximize (length (plist-get e :key))))
             (lines (append
                     (list (format " Bad Juju — %s bindings" window-type) "")
                     (cl-loop for e in entries
                              when (and (stringp (plist-get e :key))
                                        (not (string= (plist-get e :key) "")))
                              collect (format " %s%s%s"
                                              (plist-get e :key)
                                              (make-string (+ max-key
                                                              (- (length (plist-get e :key)))
                                                              3)
                                                           ?\s)
                                              (or (plist-get e :description) "")))
                     (list "")))
             (width (max 30 (cl-loop for l in lines maximize (length l))))
             (buf (get-buffer-create " *badjuju-help*")))
        (with-current-buffer buf
          (let ((inhibit-read-only t))
            (erase-buffer)
            (insert (mapconcat #'identity lines "\n"))
            (setq buffer-read-only t)))
        (let* ((win-height (frame-height))
               (win-width (frame-width))
               (rows (length lines))
               (row (max 0 (/ (- win-height rows) 2)))
               (col (max 0 (/ (- win-width width) 2))))
          (display-buffer-in-side-window
           buf
           `((side . bottom)
             (slot . 1)
             (window-height . ,rows))))
        (let ((win (get-buffer-window " *badjuju-help*")))
          (when win
            (with-selected-window win
              (local-set-key (kbd "q") #'quit-window)
              (local-set-key (kbd "?") #'quit-window)
              (local-set-key (kbd "<escape>") #'quit-window))))))))

;;; RET dispatch

(defun badjuju--ret-dispatch ()
  "Context-dispatched RET handler.
On a `JJ: <Label>: <revset>' shortcut line: apply the revset shortcut.
Otherwise: invoke `xref-find-definitions' (go-to-definition)."
  (interactive)
  (if (string-match-p "^JJ: [^:]+:" (buffer-substring-no-properties
                                       (line-beginning-position)
                                       (line-end-position)))
      (badjuju-commands-run "badjuju.log"
                            (list (badjuju-commands-cursor-arg)))
    (call-interactively #'xref-find-definitions)))

;;; Squash helpers

(defun badjuju--run-squash-commit ()
  "Begin or complete a commit-to-commit squash at point."
  (badjuju-commands-run "badjuju.squash.commit"
                        (list (badjuju-commands-cursor-arg))))

(defun badjuju--run-squash-file ()
  "Squash file at cursor into parent.
When the server returns RequiresParentSelection, prompt to pick the parent."
  (let ((cursor-arg (badjuju-commands-cursor-arg)))
    (badjuju-commands-run-with-handler
     "badjuju.squash"
     (list cursor-arg)
     (lambda (err)
       (let* ((data    (plist-get err :data))
              (code    (and (listp data) (plist-get data :code)))
              (file    (and (listp data) (plist-get data :file)))
              (cands   (and (listp data) (plist-get data :candidates))))
         (if (and (stringp code) (string= code "RequiresParentSelection")
                  cands file)
             (badjuju-squash-with-parent-prompt file (append cands nil))
           (user-error "badjuju squash: %s" (plist-get err :message))))))))

;;; Rebase helpers

(defun badjuju--run-rebase-source (mode)
  "Mark rebase source with MODE (\"source\", \"revisions\", or \"branch\")."
  (badjuju-commands-run "badjuju.rebase.source"
                        (list mode (badjuju-commands-cursor-arg))))

(defun badjuju--run-rebase-commit (insert)
  "Execute pending rebase with INSERT position (\"onto\", \"after\", \"before\")."
  (badjuju-commands-run "badjuju.rebase.commit"
                        (list insert (badjuju-commands-cursor-arg))))

(defun badjuju--run-cancel ()
  "Cancel a pending squash or rebase selection."
  (badjuju-commands-run "badjuju.cancel"
                        (list (badjuju-commands-cursor-arg))))

;;; Status & log keymaps (#39)

;; NOTE: `define-derived-mode' in badjuju-mode.el already binds each MODE-map
;; variable to an empty keymap (parented at `badjuju-mode-map').  A subsequent
;; `(defvar MAP-VAR ...)' with an initializer is a *no-op* — defvar does not
;; rebind an already-bound symbol — which would silently throw away every
;; binding below.  Populate the existing map instead, which also preserves
;; the parent-map link set up by `define-derived-mode'.

(let ((map badjuju-status-mode-map))
  ;; Navigation / refresh
  (define-key map (kbd "g")       #'badjuju-refresh)
  (define-key map (kbd "R")       #'badjuju-refresh)
  (define-key map (kbd "q")       #'bury-buffer)
  (define-key map (kbd "<tab>")   #'badjuju-keymap--fold-toggle)
  ;; Diff
  (define-key map (kbd "d")       #'badjuju-diff)
  (define-key map (kbd "D")       #'badjuju-diff-commit)
  (define-key map (kbd "=")       #'badjuju-diff)
  ;; Commit operations
  (define-key map (kbd "n")       #'badjuju-new)
  (define-key map (kbd "e")       #'badjuju-edit)
  (define-key map (kbd "a")       #'badjuju-abandon)
  (define-key map (kbd "L")       #'badjuju-log)
  ;; Squash / unsquash (swapped per #47: u=unsquash, U=undo)
  (define-key map (kbd "s")       #'badjuju--run-squash-commit)
  (define-key map (kbd "S")       #'badjuju--run-squash-file)
  (define-key map (kbd "u")       #'badjuju-unsquash)
  (define-key map (kbd "U")       #'badjuju-undo)
  ;; Rebase chords (two-step: first pick source+mode, then destination+insert)
  (define-key map (kbd "r s")     (lambda () (interactive) (badjuju--run-rebase-source "source")))
  (define-key map (kbd "r r")     (lambda () (interactive) (badjuju--run-rebase-source "revisions")))
  (define-key map (kbd "r b")     (lambda () (interactive) (badjuju--run-rebase-source "branch")))
  (define-key map (kbd "r o")     (lambda () (interactive) (badjuju--run-rebase-commit "onto")))
  (define-key map (kbd "r A")     (lambda () (interactive) (badjuju--run-rebase-commit "after")))
  (define-key map (kbd "r B")     (lambda () (interactive) (badjuju--run-rebase-commit "before")))
  ;; Cancel pending operation (squash or rebase)
  (define-key map (kbd "x")       #'badjuju--run-cancel)
  ;; Remote / bookmark
  (define-key map (kbd "f")       #'badjuju-fetch)
  (define-key map (kbd "p")       #'badjuju-push)
  (define-key map (kbd "P")       (lambda () (interactive) (badjuju-push t)))
  (define-key map (kbd "b")       #'badjuju-bookmark)
  ;; Commit transient
  (define-key map (kbd "c")       #'badjuju-commit)
  ;; Help
  (define-key map (kbd "?")       (lambda () (interactive) (badjuju--show-help "status")))
  ;; Code actions intentionally unbound here — use `M-x eglot-code-actions'.
  ;; xref-find-definitions: available via RET (below) and the global M-. binding.
  ;; `gd' (vim-style) can't coexist with `g' as a non-prefix command.
  (define-key map (kbd "RET")     #'xref-find-definitions))

(let ((map badjuju-log-mode-map))
  ;; Navigation / refresh
  (define-key map (kbd "g")       #'badjuju-refresh)
  (define-key map (kbd "R")       #'badjuju-refresh)
  (define-key map (kbd "q")       #'bury-buffer)
  ;; Diff / describe
  (define-key map (kbd "d")       #'badjuju-diff)
  (define-key map (kbd "D")       #'badjuju-diff-commit)
  (define-key map (kbd "=")       #'badjuju-diff)
  ;; Commit operations
  (define-key map (kbd "e")       #'badjuju-edit)
  (define-key map (kbd "a")       #'badjuju-abandon)
  ;; Squash
  (define-key map (kbd "s")       #'badjuju--run-squash-commit)
  (define-key map (kbd "S")       #'badjuju--run-squash-file)
  ;; Undo (no unsquash in log context)
  (define-key map (kbd "U")       #'badjuju-undo)
  ;; Rebase chords (two-step: first pick source+mode, then destination+insert)
  (define-key map (kbd "r s")     (lambda () (interactive) (badjuju--run-rebase-source "source")))
  (define-key map (kbd "r r")     (lambda () (interactive) (badjuju--run-rebase-source "revisions")))
  (define-key map (kbd "r b")     (lambda () (interactive) (badjuju--run-rebase-source "branch")))
  (define-key map (kbd "r o")     (lambda () (interactive) (badjuju--run-rebase-commit "onto")))
  (define-key map (kbd "r A")     (lambda () (interactive) (badjuju--run-rebase-commit "after")))
  (define-key map (kbd "r B")     (lambda () (interactive) (badjuju--run-rebase-commit "before")))
  ;; Cancel pending operation (squash or rebase)
  (define-key map (kbd "x")       #'badjuju--run-cancel)
  ;; Remote / bookmark
  (define-key map (kbd "b")       #'badjuju-bookmark)
  ;; Commit transient
  (define-key map (kbd "c")       #'badjuju-commit)
  ;; Help
  (define-key map (kbd "?")       (lambda () (interactive) (badjuju--show-help "log")))
  ;; Code actions intentionally unbound here — use `M-x eglot-code-actions'.
  ;; xref-find-definitions: available via the global M-. binding.
  ;; `gd' (vim-style) can't coexist with `g' as a non-prefix command.
  (define-key map (kbd "RET")     #'badjuju--ret-dispatch))

(let ((map badjuju-diff-mode-map))
  (define-key map (kbd "g")     #'badjuju-refresh)
  (define-key map (kbd "R")     #'badjuju-refresh)
  (define-key map (kbd "q")     #'bury-buffer)
  (define-key map (kbd "?")     (lambda () (interactive) (badjuju--show-help "diff")))
  ;; Code actions intentionally unbound here — use `M-x eglot-code-actions'.
  ;; xref-find-definitions: available via RET (below) and the global M-. binding.
  ;; `gd' (vim-style) can't coexist with `g' as a non-prefix command.
  (define-key map (kbd "RET")   #'xref-find-definitions))

;;; Squash & hunk-edit keymaps (#41)

(let ((map badjuju-squash-mode-map))
  (define-key map (kbd "s")     (lambda () (interactive)
                                  (badjuju-commands-run "badjuju.squash.toggle"
                                                        (list (badjuju-commands-cursor-arg)))))
  (define-key map (kbd "e")     (lambda () (interactive)
                                  (badjuju-commands-run "badjuju.squash.edit_hunk"
                                                        (list (badjuju-commands-cursor-arg)))))
  (define-key map (kbd "a")     (lambda () (interactive)
                                  (badjuju-commands-run "badjuju.squash.select_all" nil)))
  (define-key map (kbd "A")     (lambda () (interactive)
                                  (badjuju-commands-run "badjuju.squash.select_none" nil)))
  (define-key map (kbd "u")     #'badjuju-undo)
  (define-key map (kbd "<tab>") #'badjuju-keymap--fold-toggle)
  (define-key map (kbd "q")     #'bury-buffer)
  (define-key map (kbd "?")     (lambda () (interactive) (badjuju--show-help "squash")))
  (define-key map (kbd "gd")    #'xref-find-definitions))

(let ((map badjuju-hunk-edit-mode-map))
  (define-key map (kbd "C-c C-c") #'save-buffer)
  (define-key map (kbd "C-c C-k") #'bury-buffer))

;;; Fold toggle (#49)

(defun badjuju-keymap--fold-toggle ()
  "Toggle the fold at point.
Uses `outline-toggle-children' when outline-minor-mode is active; falls
back to `outline-cycle' if available."
  (interactive)
  (cond
   ((and (bound-and-true-p outline-minor-mode)
         (fboundp 'outline-toggle-children))
    (outline-toggle-children))
   ((fboundp 'outline-cycle)
    (outline-cycle))
   (t
    (message "badjuju: no fold support available"))))

;;; Folding setup for status and squash buffers (#49)

(defun badjuju-keymap--setup-folding ()
  "Configure LSP-driven folding for the current buffer.
Called from mode hooks on status and squash buffers."
  ;; Eglot (Emacs 29+) exposes eglot-managed LSP folds via imenu/outline.
  ;; Use outline-minor-mode with a regexp that matches the section headers
  ;; the server emits so TAB can fold/unfold sections.
  (setq-local outline-regexp
              "^\\(WORKING COPY CHANGES\\|PARENT CHANGES\\|STACK\\|SELECTED\\|REMAINING\\|@@\\)")
  (outline-minor-mode 1)
  ;; Auto-fold everything on first open, then expand the main sections.
  (run-with-idle-timer
   0.1 nil
   (lambda (buf)
     (when (buffer-live-p buf)
       (with-current-buffer buf
         (condition-case nil
             (progn
               (outline-hide-body)
               ;; Expand WORKING COPY CHANGES and PARENT CHANGES.
               (save-excursion
                 (goto-char (point-min))
                 (while (re-search-forward
                         "^\\(WORKING COPY CHANGES\\|PARENT CHANGES\\)" nil t)
                   (outline-show-children)
                   (outline-show-entry))))
           (error nil)))))
   (current-buffer)))

;;; Hook setup

(defun badjuju-keymap-setup ()
  "Install keymaps and folding on all badjuju mode buffers.
Add this to the appropriate mode hooks, or call it from `badjuju-mode-hook'."
  ;; Keymaps are installed via the mode variables; modes inherit them.
  ;; Folding is set up separately for the buffers that need it.
  (cond
   ((derived-mode-p 'badjuju-status-mode)
    (use-local-map badjuju-status-mode-map)
    (badjuju-keymap--setup-folding))
   ((derived-mode-p 'badjuju-log-mode)
    (use-local-map badjuju-log-mode-map))
   ((derived-mode-p 'badjuju-diff-mode)
    (use-local-map badjuju-diff-mode-map))
   ((derived-mode-p 'badjuju-squash-mode)
    (use-local-map badjuju-squash-mode-map)
    (badjuju-keymap--setup-folding))
   ((derived-mode-p 'badjuju-hunk-edit-mode)
    (use-local-map badjuju-hunk-edit-mode-map))))

(add-hook 'badjuju-mode-hook #'badjuju-keymap-setup)

(provide 'badjuju-keymap)
;;; badjuju-keymap.el ends here
