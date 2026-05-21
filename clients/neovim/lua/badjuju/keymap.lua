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
}

--- Handle <CR> on a log.jujutsu buffer. If the cursor sits on a
--- `JJ: <label>: <revset>` shortcut line, re-run badjuju.log with that
--- revset and restore the cursor (row/col clamped to the new buffer). On
--- any other line, fall through to the default <CR> behavior.
local function apply_log_shortcut()
  local line = vim.api.nvim_get_current_line()
  local _, revset = require('badjuju.log_shortcut').parse(line)
  if not revset then
    -- Pass the keypress through unchanged.
    vim.api.nvim_feedkeys(
      vim.api.nvim_replace_termcodes('<CR>', true, false, true),
      'n',
      false
    )
    return
  end

  local win = vim.api.nvim_get_current_win()
  local saved = vim.api.nvim_win_get_cursor(win) -- {row (1-based), col}

  require('badjuju').execute('badjuju.log', { revset }, {
    after = function()
      if not vim.api.nvim_win_is_valid(win) then
        return
      end
      local buf = vim.api.nvim_win_get_buf(win)
      local line_count = vim.api.nvim_buf_line_count(buf)
      local row = math.min(saved[1], line_count)
      row = math.max(row, 1)
      local text = vim.api.nvim_buf_get_lines(buf, row - 1, row, false)[1] or ''
      local col = math.min(saved[2], #text)
      vim.api.nvim_win_set_cursor(win, { row, col })
    end,
  })
end

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
    nmap(bufnr, '<CR>', apply_log_shortcut, 'badjuju: apply revset shortcut under cursor')
  end
end

return M
