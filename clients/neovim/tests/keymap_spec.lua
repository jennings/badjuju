local keymap = require('badjuju.keymap')

local counter = 0
local function unique_path(relative)
  counter = counter + 1
  return vim.fn.tempname() .. '-keymap' .. counter .. '/' .. relative
end

local function open_named(relative)
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, unique_path(relative))
  keymap.setup_for_buffer(0)
  return vim.api.nvim_get_current_buf()
end

local function has_buffer_map(bufnr, key)
  for _, m in ipairs(vim.api.nvim_buf_get_keymap(bufnr, 'n')) do
    if m.lhs == key then
      return true, m
    end
  end
  return false, nil
end

describe('keymap.setup_for_buffer', function()
  it('installs status.jj maps including g/r/n/l/d/q/u/= and a', function()
    local buf = open_named('.jj/badjuju/status.jj')
    for _, key in ipairs({ 'g', 'r', 'n', 'l', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected status.jj map for ' .. key)
    end
  end)

  it('installs log.jj maps for refresh and abandon', function()
    local buf = open_named('.jj/badjuju/log.jj')
    for _, key in ipairs({ 'g', 'r', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected log.jj map for ' .. key)
    end
  end)

  it('does NOT install status-only maps on log.jj', function()
    local buf = open_named('.jj/badjuju/log.jj')
    for _, key in ipairs({ 'n', 'l', 'd', 'q', '=', 'u' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'log.jj should not bind ' .. key)
    end
  end)

  it('does nothing for unrelated jujutsu buffers', function()
    local buf = open_named('describe.jj')
    for _, key in ipairs({ 'g', 'r', 'n', 'l', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'unrelated buffer should not bind ' .. key)
    end
  end)
end)
