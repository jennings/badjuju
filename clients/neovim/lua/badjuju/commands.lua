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

--- When invoked from a status.jujutsu or log.jujutsu buffer, return a
--- cursor-form argument the server can resolve to the commit under the
--- cursor (so :JJNew creates the new change as a child of THAT commit).
--- Returns nil from any other buffer; callers fall back to the server's
--- default of creating a child of @.
local function new_parent_cursor_arg()
  local name = vim.api.nvim_buf_get_name(0)
  if
    not name:match('/%.jj/badjuju/status%.jujutsu$')
    and not name:match('/%.jj/badjuju/log%.jujutsu$')
  then
    return nil
  end
  local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1
  return { cursor = { uri = vim.uri_from_fname(name), line = cursor_line } }
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

  cmd('JJLogFile', { nargs = '?' }, function(args)
    -- Default to the file the user is currently visiting. Inside a
    -- status.jujutsu buffer, prefer the file under the cursor (ship the
    -- cursor-form arg and let the server resolve it).
    local name = vim.api.nvim_buf_get_name(0)
    local arguments
    if name:match('/%.jj/badjuju/status%.jujutsu$') then
      local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1
      arguments = { { cursor = { uri = vim.uri_from_fname(name), line = cursor_line } } }
    else
      if name == '' then
        vim.notify('badjuju: buffer is not visiting a file', vim.log.levels.ERROR)
        return
      end
      local rel = vim.fn.fnamemodify(name, ':.')
      arguments = { rel }
    end
    if args.args ~= '' then table.insert(arguments, args.args) end
    badjuju.execute('badjuju.log.file', arguments)
  end)

  cmd('JJDescribe', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or nil
    badjuju.execute('badjuju.describe', revision and { revision } or {})
  end)

  cmd('JJDiff', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or nil
    badjuju.execute('badjuju.diff', revision and { revision } or {})
  end)

  cmd('JJDiffCommit', { nargs = '?' }, function(args)
    local revision = args.args ~= '' and args.args or nil
    badjuju.execute('badjuju.diff.commit', revision and { revision } or {})
  end)

  cmd('JJNew', {}, function()
    local arg = new_parent_cursor_arg()
    badjuju.execute('badjuju.new', arg and { arg } or {})
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

  cmd('JJBookmark', { nargs = '+' }, function(args)
    -- :JJBookmark <sub_action> <name> [revision]
    -- sub_action: create | move | delete | track | forget
    local sub_action = args.fargs[1] or ''
    local name = args.fargs[2] or ''
    local revision = args.fargs[3] or ''
    badjuju.execute('badjuju.bookmark', { sub_action, name, revision })
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
    badjuju.execute('badjuju.rebase.source', { 'source', source }, {
      after = function()
        badjuju.execute('badjuju.rebase.commit', { 'onto', dest })
      end,
    })
  end)
end

return M
