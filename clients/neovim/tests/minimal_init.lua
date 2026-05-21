-- Minimal init for headless plenary test runs.
-- The test runner script clones plenary into PLENARY_DIR and sets it in env.

local plenary_dir = vim.env.PLENARY_DIR
if not plenary_dir or plenary_dir == '' then
  io.stderr:write('PLENARY_DIR not set; cannot locate plenary.nvim\n')
  os.exit(2)
end

vim.opt.runtimepath:prepend(plenary_dir)
vim.opt.runtimepath:prepend(vim.fn.getcwd())

vim.cmd('runtime plugin/plenary.vim')
