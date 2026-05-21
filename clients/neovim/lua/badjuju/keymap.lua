local M = {}

local function nmap(bufnr, key, rhs, desc)
  vim.keymap.set('n', key, rhs, {
    buffer = bufnr,
    silent = true,
    nowait = true,
    desc = desc,
  })
end

local function map_cmd(bufnr, key, ex_command, desc)
  nmap(bufnr, key, '<Cmd>' .. ex_command .. '<CR>', desc)
end

local STATUS_MAPS = {
  { 'g', 'JJRefresh',    'badjuju: refresh' },
  { 'r', 'JJRefresh',    'badjuju: refresh' },
  { 'n', 'JJNew',        'badjuju: new change' },
  { 'l', 'JJLog',        'badjuju: open log' },
  { 'd', 'JJDescribe',   'badjuju: describe' },
  { 'u', 'JJUndo',       'badjuju: undo' },
  { '=', 'JJToggleStat', 'badjuju: toggle --stat' },
  { 'a', 'JJAbandon',    'badjuju: abandon revision' },
}

local LOG_MAPS = {
  { 'g', 'JJRefresh', 'badjuju: refresh' },
  { 'r', 'JJRefresh', 'badjuju: refresh' },
  { 'a', 'JJAbandon', 'badjuju: abandon revision' },
  -- <CR> on shortcut lines is wired by a separate ticket.
}

--- Install buffer-local keymaps for the given buffer if its name matches a
--- badjuju status.jujutsu or log.jujutsu path. No-op for any other buffer.
---@param bufnr integer?  defaults to current buffer (0)
function M.setup_for_buffer(bufnr)
  bufnr = bufnr or 0
  local name = vim.api.nvim_buf_get_name(bufnr)

  if name:match('/%.jj/badjuju/status%.jujutsu$') then
    for _, m in ipairs(STATUS_MAPS) do
      map_cmd(bufnr, m[1], m[2], m[3])
    end
    nmap(bufnr, 'q', '<Cmd>quit<CR>', 'badjuju: close window')
  elseif name:match('/%.jj/badjuju/log%.jujutsu$') then
    for _, m in ipairs(LOG_MAPS) do
      map_cmd(bufnr, m[1], m[2], m[3])
    end
  end
end

return M
