;;; badjuju-test.el --- Tests for badjuju.el command wrappers  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju)

;;; No-argument wrappers

(ert-deftest badjuju-test/status-no-args ()
  (badjuju-test--with-captured-run
    (badjuju-status))
  (should (equal badjuju-test-calls '(("badjuju.status" nil)))))

(ert-deftest badjuju-test/log-no-revset ()
  (badjuju-test--with-captured-run
    (badjuju-log))
  (should (equal badjuju-test-calls '(("badjuju.log" nil)))))

(ert-deftest badjuju-test/log-with-revset ()
  (badjuju-test--with-captured-run
    (badjuju-log "main"))
  (should (equal badjuju-test-calls '(("badjuju.log" ("main"))))))

(ert-deftest badjuju-test/undo ()
  (badjuju-test--with-captured-run
    (badjuju-undo))
  (should (equal badjuju-test-calls '(("badjuju.undo" nil)))))

(ert-deftest badjuju-test/fetch ()
  (badjuju-test--with-captured-run
    (badjuju-fetch))
  (should (equal badjuju-test-calls '(("badjuju.fetch" nil)))))

;;; Push: tri-state forceWithLease

(ert-deftest badjuju-test/push-no-force ()
  (badjuju-test--with-captured-run
    (badjuju-push nil))
  (should (equal (caar badjuju-test-calls) "badjuju.push"))
  (let ((payload (caar (cdar badjuju-test-calls))))
    (should (eq (plist-get payload :forceWithLease) :json-false))))

(ert-deftest badjuju-test/push-force ()
  (badjuju-test--with-captured-run
    (badjuju-push t))
  (should (equal (caar badjuju-test-calls) "badjuju.push"))
  (let ((payload (caar (cdar badjuju-test-calls))))
    (should (eq (plist-get payload :forceWithLease) t))))

;;; Cursor-aware wrappers: branch on derived-mode-p

(defmacro badjuju-test--in-badjuju-buffer (&rest body)
  "Evaluate BODY in a temp buffer that derives from `badjuju-mode'."
  (declare (indent 0))
  `(with-temp-buffer
     (setq buffer-file-name "/tmp/fake.jujutsu")
     (let ((default-directory "/tmp/"))
       (badjuju-status-mode)
       ,@body)))

(ert-deftest badjuju-test/new-outside-badjuju-buffer ()
  (badjuju-test--with-captured-run
    (with-temp-buffer
      (badjuju-new)))
  (should (equal badjuju-test-calls '(("badjuju.new" nil)))))

(ert-deftest badjuju-test/new-in-badjuju-buffer-sends-cursor ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer
      (badjuju-new)))
  (should (equal (caar badjuju-test-calls) "badjuju.new"))
  ;; Single argument: a (:cursor ...) plist
  (let* ((args (cadar badjuju-test-calls))
         (first (car args)))
    (should (= (length args) 1))
    (should (plist-member first :cursor))))

(ert-deftest badjuju-test/describe-outside-badjuju-buffer ()
  (badjuju-test--with-captured-run
    (with-temp-buffer (badjuju-describe)))
  ;; Outside a badjuju buffer, no cursor arg is sent
  (should (equal badjuju-test-calls '(("badjuju.describe" nil)))))

(ert-deftest badjuju-test/describe-in-badjuju-buffer ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-describe)))
  (should (equal (caar badjuju-test-calls) "badjuju.describe"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

(ert-deftest badjuju-test/diff-no-cursor-outside-buffer ()
  (badjuju-test--with-captured-run
    (with-temp-buffer (badjuju-diff)))
  (should (equal badjuju-test-calls '(("badjuju.diff" nil)))))

(ert-deftest badjuju-test/diff-with-cursor-in-buffer ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-diff)))
  (should (equal (caar badjuju-test-calls) "badjuju.diff"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

(ert-deftest badjuju-test/diff-commit-with-cursor-in-buffer ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-diff-commit)))
  (should (equal (caar badjuju-test-calls) "badjuju.diff.commit"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

(ert-deftest badjuju-test/edit-with-cursor ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-edit)))
  (should (equal (caar badjuju-test-calls) "badjuju.edit"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

;;; Abandon: defaults to "@" when outside a badjuju buffer

(ert-deftest badjuju-test/abandon-outside-buffer-defaults-to-at ()
  (badjuju-test--with-captured-run
    (with-temp-buffer (badjuju-abandon)))
  (should (equal badjuju-test-calls '(("badjuju.abandon" ("@"))))))

(ert-deftest badjuju-test/abandon-in-buffer-uses-cursor ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-abandon)))
  (should (equal (caar badjuju-test-calls) "badjuju.abandon"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

;;; Squash family

(ert-deftest badjuju-test/squash-no-cursor-outside-buffer ()
  (badjuju-test--with-captured-run
    (with-temp-buffer (badjuju-squash)))
  (should (equal badjuju-test-calls '(("badjuju.squash.commit" nil)))))

(ert-deftest badjuju-test/squash-file-in-buffer ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-squash-file)))
  (should (equal (caar badjuju-test-calls) "badjuju.squash"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

(ert-deftest badjuju-test/unsquash-in-buffer ()
  (badjuju-test--with-captured-run
    (badjuju-test--in-badjuju-buffer (badjuju-unsquash)))
  (should (equal (caar badjuju-test-calls) "badjuju.unsquash"))
  (should (plist-member (car (cadar badjuju-test-calls)) :cursor)))

;;; Refresh: passes file-URI of current buffer

(ert-deftest badjuju-test/refresh-without-file ()
  (badjuju-test--with-captured-run
    (with-temp-buffer (badjuju-refresh)))
  (should (equal badjuju-test-calls '(("badjuju.refresh" (""))))))

(ert-deftest badjuju-test/refresh-with-file ()
  (badjuju-test--with-captured-run
    (with-temp-buffer
      (setq buffer-file-name "/tmp/badjuju-status.jujutsu")
      (badjuju-refresh)))
  (should (equal (caar badjuju-test-calls) "badjuju.refresh"))
  (let ((arg (car (cadar badjuju-test-calls))))
    (should (string-prefix-p "file:///tmp/" arg))))

(provide 'badjuju-test)
;;; badjuju-test.el ends here
