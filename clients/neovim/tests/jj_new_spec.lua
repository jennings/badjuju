-- Tests for :JJNew passing the cursor-revision as the new change's parent
-- when invoked from a status.jujutsu or log.jujutsu buffer.

local commands = require('badjuju.commands')
local badjuju = require('badjuju')

local SAMPLE_STATUS = {
  'STATUS:',
  '',
  'Working copy changes:',
  'M src/main.rs',
  '',
  'STACK: ancestors(reachable(@, mutable()), 2)',
  '',
  '@  kpkzwvqm 909679d0 stephen@example.com',
  '│  (empty) (no description set)',
  '○  xorwskru 66bfbfdf stephen@example.com',
  '│  feat: a change',
}

local SAMPLE_LOG = {
  'REVSET: ::@',
  '',
  'OUTPUT:',
  '',
  '@  kpkzwvqm 909679d0 stephen@example.com',
  '│  (empty) (no description set)',
  '○  xorwskru 66bfbfdf stephen@example.com',
  '│  feat: a change',
}

local counter = 0
local function unique_path(rel)
  counter = counter + 1
  return vim.fn.tempname() .. '-jjnew' .. counter .. '/' .. rel
end

local function open_named(rel, lines)
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, unique_path(rel))
  vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
  return vim.api.nvim_get_current_buf()
end

describe(':JJNew parent-at-cursor', function()
  local captured
  local original_execute

  before_each(function()
    captured = nil
    original_execute = badjuju.execute
    badjuju.execute = function(command, arguments, _opts)
      captured = { command = command, arguments = arguments }
    end
    commands.register_all()
  end)

  after_each(function()
    badjuju.execute = original_execute
    pcall(vim.api.nvim_del_user_command, 'JJNew')
  end)

  it('passes the commit under cursor as parent in a status buffer', function()
    open_named('.jj/badjuju/status.jujutsu', SAMPLE_STATUS)
    -- Cursor on the parent commit header (1-based line 10 in SAMPLE_STATUS).
    vim.api.nvim_win_set_cursor(0, { 10, 0 })
    vim.cmd('JJNew')
    assert.are.equal('badjuju.new', captured.command)
    assert.are.same({ 'xorwskru' }, captured.arguments)
  end)

  it('passes @ as parent when cursor is on a STATUS file line', function()
    open_named('.jj/badjuju/status.jujutsu', SAMPLE_STATUS)
    -- Cursor on `M src/main.rs` (line 4 in SAMPLE_STATUS).
    vim.api.nvim_win_set_cursor(0, { 4, 0 })
    vim.cmd('JJNew')
    assert.are.same({ '@' }, captured.arguments)
  end)

  it('passes the commit under cursor as parent in a log buffer', function()
    open_named('.jj/badjuju/log.jujutsu', SAMPLE_LOG)
    -- Cursor on the parent commit (line 7 in SAMPLE_LOG).
    vim.api.nvim_win_set_cursor(0, { 7, 0 })
    vim.cmd('JJNew')
    assert.are.same({ 'xorwskru' }, captured.arguments)
  end)

  it('passes no parent when cursor is in the REVSET header of a log buffer', function()
    open_named('.jj/badjuju/log.jujutsu', SAMPLE_LOG)
    vim.api.nvim_win_set_cursor(0, { 1, 0 })
    vim.cmd('JJNew')
    -- find_log_revision returns nil → arguments table is empty.
    assert.are.same({}, captured.arguments)
  end)

  it('passes no parent when invoked outside a status/log buffer', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique_path('unrelated.txt'))
    vim.cmd('JJNew')
    assert.are.same({}, captured.arguments)
  end)
end)
