local badjuju = require('badjuju')

-- Tracks notifications so we can assert error messages without echoing them.
local notifications
local original_notify
local function capture_notify()
  notifications = {}
  original_notify = vim.notify
  vim.notify = function(msg, level)
    table.insert(notifications, { msg = msg, level = level })
  end
end
local function restore_notify()
  vim.notify = original_notify
end

local counter = 0
local function unique_tempdir()
  counter = counter + 1
  local dir = vim.fn.tempname() .. '-jjroot' .. counter
  vim.fn.mkdir(dir, 'p')
  return dir
end

local function with_cwd(dir, fn)
  local original = vim.fn.getcwd()
  vim.cmd.cd(dir)
  -- Also clear the buffer name so find_workspace_root falls back to cwd.
  vim.cmd.enew()
  local ok, err = pcall(fn)
  vim.cmd.cd(original)
  assert.is_true(ok, tostring(err))
end

describe('badjuju.find_workspace_root', function()
  it('returns nil outside any jj workspace', function()
    local dir = unique_tempdir()
    with_cwd(dir, function()
      assert.is_nil(badjuju.find_workspace_root())
    end)
  end)

  it('finds .jj walking up from cwd', function()
    local dir = unique_tempdir()
    vim.fn.mkdir(dir .. '/.jj', 'p')
    local sub = dir .. '/nested/deep'
    vim.fn.mkdir(sub, 'p')
    -- macOS tempdirs live under /var which symlinks to /private/var; vim.fs.root
    -- returns the resolved path, so normalize before comparing.
    local expected = vim.fn.resolve(dir)
    with_cwd(sub, function()
      assert.are.equal(expected, vim.fn.resolve(badjuju.find_workspace_root()))
    end)
  end)
end)

describe('badjuju.execute outside a jj workspace', function()
  before_each(capture_notify)
  after_each(restore_notify)

  it('does not spawn an LSP and reports an error', function()
    local dir = unique_tempdir()
    -- Guard: also stub vim.lsp.start so a regression would not silently
    -- spawn `badjuju lsp` during the test run.
    local start_calls = 0
    local original_start = vim.lsp.start
    vim.lsp.start = function(...)
      start_calls = start_calls + 1
      return nil
    end

    with_cwd(dir, function()
      badjuju.execute('badjuju.status')
    end)

    vim.lsp.start = original_start

    assert.are.equal(0, start_calls)
    assert.are.equal(1, #notifications)
    assert.is_truthy(notifications[1].msg:match('not in a jj workspace'))
    assert.are.equal(vim.log.levels.ERROR, notifications[1].level)
  end)
end)
