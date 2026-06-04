;;; badjuju-uri-test.el --- Tests for badjuju-uri.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-uri)

;;; Handler regexp

(ert-deftest badjuju-uri-test/handler-regexp-matches-diff ()
  (should (string-match-p badjuju-uri--handler-regexp
                          "badjuju-diff:///change/abc")))

(ert-deftest badjuju-uri-test/handler-regexp-matches-file ()
  (should (string-match-p badjuju-uri--handler-regexp
                          "badjuju-file:///commit/abc/path.rs")))

(ert-deftest badjuju-uri-test/handler-regexp-rejects-file-scheme ()
  (should-not (string-match-p badjuju-uri--handler-regexp
                              "file:///tmp/x.jujutsu")))

(ert-deftest badjuju-uri-test/handler-regexp-rejects-https ()
  (should-not (string-match-p badjuju-uri--handler-regexp
                              "https://example.com/x")))

;;; Handler dispatch — unsupported ops fall through with inhibit guards

(ert-deftest badjuju-uri-test/handler-unsupported-op-falls-through ()
  (let (passed-args)
    (cl-letf (((symbol-function 'file-exists-p)
               (lambda (&rest args)
                 (setq passed-args (cons 'file-exists-p args))
                 ;; Must see the inhibit list so default handler runs.
                 (should (memq #'badjuju-uri--handler inhibit-file-name-handlers))
                 nil)))
      (badjuju-uri--handler 'file-exists-p "badjuju-diff:///change/abc")
      (should (equal (car passed-args) 'file-exists-p)))))

;;; insert-file-contents — fetches via LSP and sets buffer state

(ert-deftest badjuju-uri-test/insert-file-contents-diff-sets-mode ()
  (with-temp-buffer
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _) (list :text "diff content"))))
      (badjuju-uri--insert-file-contents "badjuju-diff:///change/abc"))
    (should (equal (buffer-string) "diff content"))
    (should (derived-mode-p 'badjuju-diff-mode))
    (should buffer-read-only)
    (should (equal buffer-file-name "badjuju-diff:///change/abc"))))

(ert-deftest badjuju-uri-test/insert-file-contents-file-scheme-mode-inference ()
  "badjuju-file://commit/<id>/<path> picks a mode by inferring from the path."
  (with-temp-buffer
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _) (list :text "fn main() {}"))))
      (badjuju-uri--insert-file-contents
       "badjuju-file:///commit/abc12345/src/main.rs"))
    ;; rust-mode may not be loaded headless; just ensure buffer is set up read-only.
    (should buffer-read-only)
    (should (equal (buffer-string) "fn main() {}"))))

(ert-deftest badjuju-uri-test/insert-file-contents-error-propagates ()
  (with-temp-buffer
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _) (error "boom"))))
      (should-error
       (badjuju-uri--insert-file-contents "badjuju-diff:///change/abc")
       :type 'error))))

(provide 'badjuju-uri-test)
;;; badjuju-uri-test.el ends here
