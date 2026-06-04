;;; badjuju-transient-test.el --- Tests for badjuju-transient.el  -*- lexical-binding: t; -*-

;;; Code:

(require 'ert)
(require 'transient)
(require 'badjuju-transient)

(ert-deftest badjuju-transient-test/commit-is-transient-prefix ()
  "`badjuju-commit' must be defined as a transient prefix."
  (should (commandp 'badjuju-commit))
  (should (get 'badjuju-commit 'transient--prefix)))

(provide 'badjuju-transient-test)
;;; badjuju-transient-test.el ends here
