;;; badjuju-eglot-test.el --- Tests for badjuju-eglot.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'cl-lib)
(require 'test-helpers)
(require 'badjuju-eglot)

;;; initializationOptions

(ert-deftest badjuju-eglot-test/init-options-default-profile ()
  (let ((badjuju-keymap-profile "magit")
        (badjuju-binary-path ""))
    (let ((opts (badjuju-eglot--init-options)))
      (should (equal (plist-get opts :keymapProfile) "magit"))
      (should (eq (plist-get opts :virtualDiffs) t))
      (should (not (plist-member opts :binaryPath))))))

(ert-deftest badjuju-eglot-test/init-options-none-profile ()
  (let ((badjuju-keymap-profile "none")
        (badjuju-binary-path ""))
    (let ((opts (badjuju-eglot--init-options)))
      (should (equal (plist-get opts :keymapProfile) "none")))))

(ert-deftest badjuju-eglot-test/init-options-empty-profile-fallback ()
  (let ((badjuju-keymap-profile "")
        (badjuju-binary-path ""))
    (let ((opts (badjuju-eglot--init-options)))
      (should (equal (plist-get opts :keymapProfile) "magit")))))

(ert-deftest badjuju-eglot-test/init-options-with-binary-path ()
  (let ((badjuju-keymap-profile "magit")
        (badjuju-binary-path "/usr/local/bin/badjuju"))
    (let ((opts (badjuju-eglot--init-options)))
      (should (equal (plist-get opts :binaryPath) "/usr/local/bin/badjuju")))))

;;; Workspace root discovery

(ert-deftest badjuju-eglot-test/find-workspace-root-from-root ()
  (badjuju-test--with-tempdir-repo root
    (should (equal (file-name-as-directory (badjuju--find-workspace-root root))
                   (file-name-as-directory root)))))

(ert-deftest badjuju-eglot-test/find-workspace-root-from-subdir ()
  (badjuju-test--with-tempdir-repo root
    (let* ((sub (expand-file-name "a/b/c/" root)))
      (make-directory sub t)
      (should (equal (file-name-as-directory (badjuju--find-workspace-root sub))
                     (file-name-as-directory root))))))

(ert-deftest badjuju-eglot-test/find-workspace-root-outside-repo ()
  (let* ((dir (file-name-as-directory (make-temp-file "badjuju-norepo-" t))))
    (unwind-protect
        (should (null (badjuju--find-workspace-root dir)))
      (delete-directory dir t))))

;;; project-find

(ert-deftest badjuju-eglot-test/project-find-returns-cons-inside-repo ()
  (badjuju-test--with-tempdir-repo root
    (let ((proj (badjuju--project-find root)))
      (should (consp proj))
      (should (eq (car proj) 'badjuju))
      (should (file-directory-p (cdr proj))))))

(ert-deftest badjuju-eglot-test/project-find-nil-outside-repo ()
  (let* ((dir (file-name-as-directory (make-temp-file "badjuju-norepo-" t))))
    (unwind-protect
        (should (null (badjuju--project-find dir)))
      (delete-directory dir t))))

;;; ensure-server: errors outside a jj repo

(ert-deftest badjuju-eglot-test/ensure-server-errors-outside-repo ()
  (let* ((dir (file-name-as-directory (make-temp-file "badjuju-norepo-" t)))
         (default-directory dir))
    (unwind-protect
        (should-error (badjuju--ensure-server) :type 'user-error)
      (delete-directory dir t))))

(provide 'badjuju-eglot-test)
;;; badjuju-eglot-test.el ends here
