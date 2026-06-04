;;; test-helpers.el --- Test helpers for badjuju ERT suite  -*- lexical-binding: t; -*-

;;; Commentary:

;; Macros that absorb the LSP plumbing so unit tests stay terse.
;;
;;   `badjuju-test--with-captured-run' — stub `badjuju-commands-run' and
;;     `badjuju-commands-run-with-handler', record each call as (COMMAND ARGS)
;;     in the symbol bound to RECORDED.
;;
;;   `badjuju-test--with-mock-server' — stub `badjuju--ensure-server' and
;;     `jsonrpc-request' with canned responses.  RESPONSES is a list of
;;     (METHOD COMMAND-OR-NIL . RESULT) tuples; the first match wins.
;;
;;   `badjuju-test--with-mock-input' — stub `read-string', `completing-read',
;;     `read-from-minibuffer' and `y-or-n-p' with a queue of answers.
;;
;;   `badjuju-test--with-tempdir-repo' — create a real jj git repo under a
;;     `make-temp-file' dir, evaluate BODY with `default-directory' bound,
;;     and clean up unconditionally.

;;; Code:

(require 'cl-lib)
(require 'ert)

(defvar badjuju-test-calls nil
  "List of (COMMAND ARGS) calls captured by
`badjuju-test--with-captured-run'.  Reset on each invocation.")

(defmacro badjuju-test--with-captured-run (&rest body)
  "Run BODY with `badjuju-commands-run' / `-run-with-handler' stubbed.
Each captured call is appended to `badjuju-test-calls' as
\(COMMAND ARGS).  The list is reset on entry and exposed in oldest-first
order on exit."
  (declare (indent 0))
  `(progn
     (setq badjuju-test-calls nil)
     (cl-letf* (((symbol-function 'badjuju-commands-run)
                 (lambda (command &optional args)
                   (push (list command args) badjuju-test-calls)
                   nil))
                ((symbol-function 'badjuju-commands-run-with-handler)
                 (lambda (command args _on-error)
                   (push (list command args) badjuju-test-calls)
                   nil)))
       ,@body)
     (setq badjuju-test-calls (nreverse badjuju-test-calls))))

(defmacro badjuju-test--with-mock-server (responses &rest body)
  "Stub `badjuju--ensure-server' and `jsonrpc-request' during BODY.
RESPONSES is an alist of (METHOD COMMAND-OR-NIL . RESULT).  Each
`jsonrpc-request' call is matched against METHOD; when COMMAND-OR-NIL
is non-nil, the request must be a workspace/executeCommand whose
:command field equals COMMAND-OR-NIL.  The first matching tuple's
RESULT is returned.  Unmatched calls raise `error'."
  (declare (indent 1))
  `(let ((badjuju-test--mock-responses ,responses)
         (badjuju-test--mock-calls nil))
     (cl-letf* (((symbol-function 'badjuju--ensure-server)
                 (lambda () 'mock-server))
                ((symbol-function 'jsonrpc-request)
                 (lambda (_server method params &rest _)
                   (push (list method params) badjuju-test--mock-calls)
                   (let ((command (and (listp params)
                                       (plist-get params :command))))
                     (or (cl-loop for resp in badjuju-test--mock-responses
                                  for m = (nth 0 resp)
                                  for c = (nth 1 resp)
                                  for r = (cdr (cdr resp))
                                  when (and (eq m method)
                                            (or (null c) (equal c command)))
                                  return (car r))
                         (error "mock-server: no canned response for %s %s"
                                method command))))))
       (setq badjuju-test--mock-calls nil)
       ,@body)))

(defvar badjuju-test--mock-responses nil
  "Dynamic binding used by `badjuju-test--with-mock-server'.")

(defvar badjuju-test--mock-calls nil
  "List of jsonrpc calls captured during a `badjuju-test--with-mock-server' block.")

(defmacro badjuju-test--with-mock-input (answers &rest body)
  "Stub minibuffer reads with ANSWERS, a list consumed in order.
Each call to `read-string', `completing-read', `read-from-minibuffer'
or `y-or-n-p' pops the next element of ANSWERS.  An empty queue
raises an error so tests notice missing fixtures.

`y-or-n-p' coerces non-string truthy answers to t."
  (declare (indent 1))
  `(let ((badjuju-test--input-queue (copy-sequence ,answers)))
     (cl-letf* ((take (lambda (_prompt &rest _)
                        (unless badjuju-test--input-queue
                          (error "mock-input: queue empty"))
                        (pop badjuju-test--input-queue)))
                ((symbol-function 'read-string) take)
                ((symbol-function 'read-from-minibuffer) take)
                ((symbol-function 'completing-read)
                 (lambda (_prompt _coll &rest _)
                   (unless badjuju-test--input-queue
                     (error "mock-input: queue empty"))
                   (pop badjuju-test--input-queue)))
                ((symbol-function 'y-or-n-p)
                 (lambda (_prompt)
                   (unless badjuju-test--input-queue
                     (error "mock-input: queue empty"))
                   (and (pop badjuju-test--input-queue) t))))
       ,@body)))

(defvar badjuju-test--input-queue nil
  "Dynamic binding used by `badjuju-test--with-mock-input'.")

(defmacro badjuju-test--with-tempdir-repo (root-var &rest body)
  "Create a fresh jj git repo and bind ROOT-VAR to its absolute path.
The repo is initialised with `jj git init'.  `default-directory' is
bound to ROOT-VAR for the duration of BODY.  The directory is removed
in an `unwind-protect' even on failure."
  (declare (indent 1))
  `(let* ((,root-var (file-name-as-directory
                      (make-temp-file "badjuju-test-" t)))
          (default-directory ,root-var))
     (unwind-protect
         (progn
           (let ((rc (call-process "jj" nil nil nil "git" "init")))
             (unless (zerop rc)
               (error "jj git init failed (rc=%s) in %s" rc ,root-var)))
           ,@body)
       (when (and (stringp ,root-var) (file-directory-p ,root-var))
         (delete-directory ,root-var t)))))

(defun badjuju-test--make-fake-jj-file (root relpath)
  "Create an empty file at ROOT/RELPATH and return its absolute path.
Parent directories are created as needed."
  (let ((abs (expand-file-name relpath root)))
    (make-directory (file-name-directory abs) t)
    (with-temp-file abs (insert ""))
    abs))

(provide 'test-helpers)
;;; test-helpers.el ends here
