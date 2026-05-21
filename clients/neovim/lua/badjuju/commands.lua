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

  cmd('JJNew', {}, function()
    badjuju.execute('badjuju.new')
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
end

return M
