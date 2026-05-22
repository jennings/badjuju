local badjuju = require('badjuju')

local M = {}

local function buffer_uri(bufnr)
  local name = vim.api.nvim_buf_get_name(bufnr or 0)
  if name == '' then
    return nil
  end
  return vim.uri_from_fname(name)
end

local function is_jujutsu_buffer(bufnr)
  return vim.bo[bufnr or 0].filetype == 'jujutsu'
end

--- When invoked from a status.jujutsu or log.jujutsu buffer, return the
--- revision of the commit under the cursor (so :JJNew creates the new change
--- as a child of THAT commit). Returns nil from any other buffer or when the
--- cursor in a log buffer isn't on a commit line — callers fall back to the
--- server's default of creating a child of @.
local function new_parent_at_cursor()
  local name = vim.api.nvim_buf_get_name(0)
  local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local parse = require('badjuju.parse')
  if name:match('/%.jj/badjuju/status%.jujutsu$') then
    return parse.find_revision_for_line(lines, cursor_line)
  elseif name:match('/%.jj/badjuju/log%.jujutsu$') then
    return parse.find_log_revision(lines, cursor_line)
  end
  return nil
end

local function cmd(name, opts, fn)
  vim.api.nvim_create_user_command(name, fn, opts)
end

function M.register_all()
  cmd('JJStatus', {}, function()
    badjuju.execute('badjuju.status')
  end)

  cmd('JJLog', { nargs = '?' }, function(args)
    local revset = args.args ~= '' and args.args or badjuju.config.default_log_revset
    local arguments = (revset and revset ~= '') and { revset } or {}
    badjuju.execute('badjuju.log', arguments)
  end)

  cmd('JJDescribe', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or nil
    badjuju.execute('badjuju.describe', revision and { revision } or {})
  end)

  cmd('JJDiff', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or nil
    badjuju.execute('badjuju.diff', revision and { revision } or {})
  end)

  cmd('JJNew', {}, function()
    local parent = new_parent_at_cursor()
    badjuju.execute('badjuju.new', parent and { parent } or {})
  end)

  cmd('JJRefresh', {}, function()
    local uri = is_jujutsu_buffer(0) and buffer_uri(0) or ''
    badjuju.execute('badjuju.refresh', { uri })
  end)

  cmd('JJSquash', { nargs = '*' }, function(args)
    badjuju.execute('badjuju.squash', args.fargs)
  end)

  cmd('JJUnsquash', { nargs = '*' }, function(args)
    badjuju.execute('badjuju.unsquash', args.fargs)
  end)

  cmd('JJToggleStat', {}, function()
    badjuju.execute('badjuju.toggleStat')
  end)

  cmd('JJUndo', {}, function()
    badjuju.execute('badjuju.undo')
  end)

  cmd('JJAbandon', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or '@'
    badjuju.execute('badjuju.abandon', { revision })
  end)

  cmd('JJEdit', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or '@'
    badjuju.execute('badjuju.edit', { revision })
  end)

  cmd('JJFetch', {}, function()
    badjuju.execute('badjuju.fetch')
  end)

  cmd('JJPush', { bang = true }, function(args)
    local force_with_lease = args.bang
    badjuju.execute('badjuju.push', { { forceWithLease = force_with_lease } })
  end)

  cmd('JJRebase', { nargs = '+' }, function(args)
    local source, dest
    if #args.fargs == 1 then
      source = '@'
      dest = args.fargs[1]
    else
      source = args.fargs[1]
      dest = args.fargs[2]
    end
    badjuju.execute('badjuju.rebase', { source, dest })
  end)
end

return M
