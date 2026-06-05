;;; badjuju-prompts-test.el --- Tests for badjuju-prompts.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-prompts)

;;; Bookmark

(ert-deftest badjuju-prompts-test/bookmark-create-with-cursor ()
  (badjuju-test--with-captured-run
    ;; sub-action then name
    (badjuju-test--with-mock-input '("create" "feature-x")
      (with-temp-buffer
        (setq buffer-file-name "/tmp/x.jujutsu")
        (badjuju-status-mode)
        (badjuju-bookmark))))
  (should (equal (caar badjuju-test-calls) "badjuju.bookmark"))
  (let ((args (cadar badjuju-test-calls)))
    (should (equal (nth 0 args) "create"))
    (should (equal (nth 1 args) "feature-x"))
    (should (plist-member (nth 2 args) :cursor))))

(ert-deftest badjuju-prompts-test/bookmark-delete-skips-cursor ()
  (badjuju-test--with-captured-run
    (badjuju-test--with-mock-input '("delete" "old-branch")
      (with-temp-buffer
        (setq buffer-file-name "/tmp/x.jujutsu")
        (badjuju-status-mode)
        (badjuju-bookmark))))
  (let ((args (cadar badjuju-test-calls)))
    (should (equal (nth 0 args) "delete"))
    (should (equal (nth 2 args) ""))))

(ert-deftest badjuju-prompts-test/bookmark-empty-name-noop ()
  (badjuju-test--with-captured-run
    (badjuju-test--with-mock-input '("create" "")
      (with-temp-buffer (badjuju-bookmark))))
  (should (null badjuju-test-calls)))

;;; Multi-parent squash disambiguation

(ert-deftest badjuju-prompts-test/squash-with-parent-prompt-picks-id ()
  (let ((candidates '((:label "abc Foo" :id "abc12345")
                      (:label "def Bar" :id "def67890")
                      (:label "ghi Baz" :id "ghi00000"))))
    (badjuju-test--with-captured-run
      ;; pick second candidate
      (badjuju-test--with-mock-input '("def Bar")
        (badjuju-squash-with-parent-prompt "src/x.rs" candidates)))
    (should (equal (caar badjuju-test-calls) "badjuju.squash.into"))
    (let* ((payload (car (cadar badjuju-test-calls)))
           (parent-id (plist-get payload :parentId))
           (file (plist-get payload :file)))
      (should (equal file "src/x.rs"))
      (should (equal parent-id "def67890")))))

(ert-deftest badjuju-prompts-test/squash-with-parent-prompt-first-index-zero ()
  "cl-position indexing regression net (off-by-one would pick wrong id)."
  (let ((candidates '((:label "first" :id "id-first")
                      (:label "second" :id "id-second"))))
    (badjuju-test--with-captured-run
      (badjuju-test--with-mock-input '("first")
        (badjuju-squash-with-parent-prompt "f" candidates)))
    (should (equal (plist-get (car (cadar badjuju-test-calls)) :parentId) "id-first"))))

(provide 'badjuju-prompts-test)
;;; badjuju-prompts-test.el ends here
