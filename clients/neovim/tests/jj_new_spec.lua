-- Tests for :JJNew shipping a cursor-form argument when invoked from a
-- status.jujutsu or log.jujutsu buffer, so the server resolves the parent
-- commit under the cursor.

local commands = require('badjuju.commands')
local badjuju = require('badjuju')

local counter = 0
local function unique_path(rel)
  counter = counter + 1
  return vim.fn.tempname() .. '-jjnew' .. counter .. '/' .. rel
end

local function open_named(rel, lines)
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, unique_path(rel))
  if lines then
    vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
  end
  return vim.api.nvim_get_current_buf()
end

-- A handful of placeholder lines so cursor positions exercised below are in range.
local FILLER = { 'line1', 'line2', 'line3', 'line4', 'line5', 'line6' }

describe(':JJNew cursor-form arg', function()
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

  it('sends cursor-form for status.jujutsu buffer', function()
    open_named('.jj/badjuju/status.jujutsu', FILLER)
    vim.api.nvim_win_set_cursor(0, { 3, 0 })
    vim.cmd('JJNew')
    assert.are.equal('badjuju.new', captured.command)
    assert.are.equal(1, #captured.arguments)
    local arg = captured.arguments[1]
    assert.are.equal(2, arg.cursor.line)
    assert.is_truthy(arg.cursor.uri:match('/status%.jujutsu$'))
  end)

  it('sends cursor-form for log.jujutsu buffer', function()
    open_named('.jj/badjuju/log.jujutsu', FILLER)
    vim.api.nvim_win_set_cursor(0, { 5, 0 })
    vim.cmd('JJNew')
    assert.are.equal('badjuju.new', captured.command)
    local arg = captured.arguments[1]
    assert.are.equal(4, arg.cursor.line)
    assert.is_truthy(arg.cursor.uri:match('/log%.jujutsu$'))
  end)

  it('sends no arguments when invoked outside a status/log buffer', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique_path('unrelated.txt'))
    vim.cmd('JJNew')
    assert.are.same({}, captured.arguments)
  end)
end)
