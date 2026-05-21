-- Verifies the ftplugin/jujutsu.lua per-buffer behavior. The ftplugin is
-- sourced manually after the buffer is named so the test doesn't depend on
-- `:filetype plugin on` or implicit autocmds.

local counter = 0
local function unique(prefix)
  counter = counter + 1
  return prefix .. counter
end

local function make_buffer(relative)
  -- Each call gets a fresh buffer with a unique parent directory so neovim
  -- doesn't refuse the rename with E95: buffer with this name already exists.
  local path = vim.fn.tempname() .. '-' .. unique('buf') .. '/' .. relative
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, path)
  vim.bo.modifiable = true
  vim.bo.readonly = false
  vim.cmd('runtime ftplugin/jujutsu.lua')
end

describe('ftplugin/jujutsu.lua', function()
  it('sets commentstring on any jujutsu buffer', function()
    make_buffer('anything.jujutsu')
    assert.are.equal('JJ: %s', vim.bo.commentstring)
  end)

  it('makes status.jujutsu buffers read-only', function()
    make_buffer('.jj/badjuju/status.jujutsu')
    assert.is_false(vim.bo.modifiable)
    assert.is_true(vim.bo.readonly)
  end)

  it('leaves log.jujutsu buffers modifiable (REVSET header is editable)', function()
    make_buffer('.jj/badjuju/log.jujutsu')
    assert.is_true(vim.bo.modifiable)
    assert.is_false(vim.bo.readonly)
  end)

  it('leaves describe.jujutsu buffers modifiable', function()
    make_buffer('.jj/badjuju/describe.jujutsu')
    assert.is_true(vim.bo.modifiable)
  end)

  it('enables autoread on jujutsu buffers', function()
    make_buffer('.jj/badjuju/status.jujutsu')
    assert.is_true(vim.bo.autoread)
  end)
end)
