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
  it('installs status.jujutsu maps including g/r/n/L/d/q/u/= and a', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    for _, key in ipairs({ 'g', 'r', 'n', 'L', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected status.jujutsu map for ' .. key)
    end
  end)

  it('installs log.jujutsu maps for refresh, abandon, describe, and diff', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    for _, key in ipairs({ 'g', 'r', 'a', 'd', 'D' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected log.jujutsu map for ' .. key)
    end
  end)

  it('installs D for diff on status.jujutsu', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found = has_buffer_map(buf, 'D')
    assert.is_true(found, 'expected status.jujutsu map for D')
  end)

  it('installs diff.jujutsu maps for refresh and close', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    for _, key in ipairs({ 'g', 'r', 'q' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected diff.jujutsu map for ' .. key)
    end
  end)

  it('does NOT install commit-action maps on diff.jujutsu', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    for _, key in ipairs({ 'n', 'L', 'd', 'D', 'a', '=', 'u', 's', 'U' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'diff.jujutsu should not bind ' .. key)
    end
  end)

  it('installs <CR> on log.jujutsu for revset shortcut application', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    local found = has_buffer_map(buf, '<CR>')
    assert.is_true(found, 'expected log.jujutsu map for <CR>')
  end)

  it('does NOT install <CR> on status.jujutsu', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found = has_buffer_map(buf, '<CR>')
    assert.is_false(found, 'status.jujutsu should not bind <CR>')
  end)

  it('does NOT install status-only maps on log.jujutsu', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    for _, key in ipairs({ 'n', 'L', 'q', '=', 'u' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'log.jujutsu should not bind ' .. key)
    end
  end)

  it('does nothing for unrelated jujutsu buffers', function()
    local buf = open_named('describe.jujutsu')
    for _, key in ipairs({ 'g', 'r', 'n', 'L', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'unrelated buffer should not bind ' .. key)
    end
  end)
end)
