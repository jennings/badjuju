;;; badjuju-commands-test.el --- Tests for badjuju-commands.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-commands)

;;; cursor-arg

(ert-deftest badjuju-commands-test/cursor-arg-zero-indexed-line ()
  "Cursor :line is 0-indexed (regression net — server expects 0-based)."
  (with-temp-buffer
    (insert "alpha\nbeta\ngamma\n")
    (goto-char (point-min))
    (let* ((arg (badjuju-commands-cursor-arg))
           (cur (plist-get arg :cursor)))
      (should (eq (plist-get cur :line) 0)))
    ;; Move to line 3 — should report :line 2.
    (goto-char (point-min))
    (forward-line 2)
    (let* ((arg (badjuju-commands-cursor-arg))
           (cur (plist-get arg :cursor)))
      (should (eq (plist-get cur :line) 2)))))

(ert-deftest badjuju-commands-test/cursor-arg-shape ()
  (with-temp-buffer
    (setq buffer-file-name "/tmp/x.jujutsu")
    (let* ((arg (badjuju-commands-cursor-arg))
           (cur (plist-get arg :cursor)))
      (should (plist-member arg :cursor))
      (should (plist-member cur :uri))
      (should (plist-member cur :line))
      (should (string-prefix-p "file://" (plist-get cur :uri))))))

(ert-deftest badjuju-commands-test/cursor-arg-no-file ()
  (with-temp-buffer
    (let* ((arg (badjuju-commands-cursor-arg))
           (cur (plist-get arg :cursor)))
      (should (equal (plist-get cur :uri) "")))))

;;; --open-uri dispatch

(ert-deftest badjuju-commands-test/open-uri-virtual-diff ()
  (let (opened)
    (cl-letf (((symbol-function 'find-file)
               (lambda (x) (push (list 'find-file x) opened))))
      (badjuju-commands--open-uri "badjuju-diff:///change/abc"))
    (should (equal opened '((find-file "badjuju-diff:///change/abc"))))))

(ert-deftest badjuju-commands-test/open-uri-virtual-file ()
  (let (opened)
    (cl-letf (((symbol-function 'find-file)
               (lambda (x) (push (list 'find-file x) opened))))
      (badjuju-commands--open-uri "badjuju-file:///commit/abc/foo.rs"))
    (should (equal opened '((find-file "badjuju-file:///commit/abc/foo.rs"))))))

(ert-deftest badjuju-commands-test/open-uri-file-scheme-decodes ()
  (let (opened)
    (cl-letf (((symbol-function 'find-file)
               (lambda (x) (push (list 'find-file x) opened))))
      (badjuju-commands--open-uri "file:///tmp/badjuju/status.jujutsu"))
    (should (equal opened '((find-file "/tmp/badjuju/status.jujutsu"))))))

(ert-deftest badjuju-commands-test/open-uri-unknown-scheme-messages ()
  (let (msg)
    (cl-letf (((symbol-function 'message)
               (lambda (&rest args) (setq msg (apply #'format args)))))
      (badjuju-commands--open-uri "https://example.com/x"))
    (should (string-match-p "unexpected URI" msg))))

;;; run / request integrate with jsonrpc-request

(ert-deftest badjuju-commands-test/run-passes-args-as-vector ()
  (let (captured)
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock-server))
              ((symbol-function 'jsonrpc-request)
               (lambda (_srv method params &rest _)
                 (setq captured (list method params))
                 ""))
              ((symbol-function 'badjuju-commands--open-uri)
               (lambda (_) nil)))
      (badjuju-commands-run "badjuju.foo" (list "a" "b"))
      (should (eq (nth 0 captured) 'workspace/executeCommand))
      (let ((args (plist-get (nth 1 captured) :arguments)))
        (should (vectorp args))
        (should (equal (aref args 0) "a"))
        (should (equal (aref args 1) "b"))))))

(ert-deftest badjuju-commands-test/run-nil-args-becomes-empty-vector ()
  (let (captured)
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock-server))
              ((symbol-function 'jsonrpc-request)
               (lambda (_srv _method params &rest _)
                 (setq captured params)
                 ""))
              ((symbol-function 'badjuju-commands--open-uri)
               (lambda (_) nil)))
      (badjuju-commands-run "badjuju.foo" nil)
      (let ((args (plist-get captured :arguments)))
        (should (vectorp args))
        (should (= (length args) 0))))))

(ert-deftest badjuju-commands-test/run-opens-returned-uri ()
  (let (opened)
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock-server))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _) "badjuju-diff:///change/xyz"))
              ((symbol-function 'find-file)
               (lambda (uri) (push uri opened))))
      (badjuju-commands-run "badjuju.diff" nil))
    (should (equal opened '("badjuju-diff:///change/xyz")))))

(ert-deftest badjuju-commands-test/run-with-handler-extracts-error-plist ()
  ;; jsonrpc.el signals as: (jsonrpc-error MSG :jsonrpc-error-code C
  ;;                                       :jsonrpc-error-message M
  ;;                                       :jsonrpc-error-data D)
  ;; — a flat list, so the wrapper takes `(cddr err)' to skip past the
  ;; (jsonrpc-error MSG) head and treat the rest as a plist.
  (let (handled)
    (cl-letf (((symbol-function 'badjuju--ensure-server)
               (lambda () 'mock-server))
              ((symbol-function 'jsonrpc-request)
               (lambda (&rest _)
                 (signal 'jsonrpc-error
                         (list "boom"
                               :jsonrpc-error-code -32000
                               :jsonrpc-error-message "no good"
                               :jsonrpc-error-data
                               (list :code "RequiresParentSelection"
                                     :file "x.rs"))))))
      (badjuju-commands-run-with-handler "badjuju.squash" nil
                                         (lambda (err) (setq handled err))))
    (should (eq (plist-get handled :code) -32000))
    (should (equal (plist-get handled :message) "no good"))
    (should (equal (plist-get (plist-get handled :data) :code)
                   "RequiresParentSelection"))))

(provide 'badjuju-commands-test)
;;; badjuju-commands-test.el ends here
