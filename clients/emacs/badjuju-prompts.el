;;; badjuju-prompts.el --- Interactive prompts for Bad Juju  -*- lexical-binding: t; -*-

;;; Commentary:

;; Minibuffer prompts for commands that need extra input beyond what the
;; cursor can resolve.
;;
;; badjuju-bookmark  — sub-action + name + revision via completing-read
;; badjuju-squash    — RequiresParentSelection multi-parent disambiguation
;;
;; The prompts are pure-client; no new server protocol is involved.

;;; Code:

(require 'badjuju-commands)

;;; Bookmark prompt

(defconst badjuju--bookmark-actions
  '("create" "move" "delete" "track" "forget")
  "Valid sub-actions for `jj bookmark'.")

;;;###autoload
(defun badjuju-bookmark ()
  "Interactively create, move, delete, track, or forget a bookmark."
  (interactive)
  (let* ((sub-action (completing-read "jj bookmark: "
                                      badjuju--bookmark-actions nil t))
         (prompt (if (string= sub-action "track")
                     "Bookmark (e.g. main@origin): "
                   "Bookmark name: "))
         (name (read-string prompt)))
    (when (and sub-action name (not (string= name "")))
      (let ((rev-arg (if (and (derived-mode-p 'badjuju-mode)
                              (member sub-action '("create" "move")))
                         (badjuju-commands-cursor-arg)
                       "")))
        (badjuju-commands-run "badjuju.bookmark"
                              (list sub-action name rev-arg))))))

;;; Multi-parent squash disambiguation

(defun badjuju-squash-with-parent-prompt (file candidates)
  "Prompt the user to pick a parent from CANDIDATES and squash FILE into it.
CANDIDATES is a list of plists with :label and :id keys (from the server's
RequiresParentSelection error data)."
  (let* ((labels (mapcar (lambda (c) (plist-get c :label)) candidates))
         (chosen (completing-read "Squash into parent: " labels nil t))
         (idx (cl-position chosen labels :test #'string=))
         (parent-id (plist-get (nth idx candidates) :id)))
    (badjuju-commands-run "badjuju.squash.into"
                          (list (list :file file :parentId parent-id)))))

(provide 'badjuju-prompts)
;;; badjuju-prompts.el ends here
