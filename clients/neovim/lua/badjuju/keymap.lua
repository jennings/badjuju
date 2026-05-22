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
  { 'R', 'JJRefresh',    'badjuju: refresh' },
  { 'n', 'JJNew',        'badjuju: new change' },
  { 'l', 'JJLog',        'badjuju: open log' },
  { 'u', 'JJUndo',       'badjuju: undo' },
  { '=', 'JJToggleStat', 'badjuju: toggle --stat' },
  { 'a', 'JJAbandon',    'badjuju: abandon revision' },
}

--- Resolve the revision under the cursor on the current jujutsu buffer.
--- - status: returns the cursor's commit, defaulting to `@` (working copy)
---   when there is no commit context (matches find_revision_for_line).
--- - log: returns the cursor's commit. Returns nil if the cursor isn't on or
---   below a commit header line; the caller should notify and bail.
---@param buffer 'status'|'log'
---@return string?
local function revision_at_cursor(buffer)
  local parse = require('badjuju.parse')
  local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1 -- 0-indexed
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  if buffer == 'log' then
    return parse.find_log_revision(lines, cursor_line)
  end
  return parse.find_revision_for_line(lines, cursor_line)
end

--- Run a server command for the commit under the cursor, opening the result
--- in a horizontal split. On log.jujutsu, a missing cursor revision triggers a
--- notification and the command is not sent.
---@param buffer 'status'|'log'
---@param server_command string  badjuju.* command name
---@param label string  human-readable label for the notification on miss
local function run_at_cursor_split(buffer, server_command, label)
  local revision = revision_at_cursor(buffer)
  if not revision then
    vim.notify(label .. ': place cursor on a commit line', vim.log.levels.INFO)
    return
  end
  require('badjuju').execute(server_command, { revision }, { split = 'h' })
end

--- Run a file-scoped status command. The cursor line is parsed for a file
--- path; if found, the command is invoked with [file, revision] where
--- revision is the commit that owns the file under the cursor (see
--- parse.find_revision_for_line). If the cursor isn't on a file line, a
--- notification is shown and the command is not sent.
---@param server_command 'badjuju.squash'|'badjuju.unsquash'
local function run_file_scoped(server_command)
  local parse = require('badjuju.parse')
  local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1 -- 0-indexed
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local line_text = lines[cursor_line + 1] or ''

  local file = parse.parse_status_file(line_text)
  if not file then
    local label = server_command == 'badjuju.squash' and 'squash' or 'unsquash'
    vim.notify(
      label .. ': place cursor on a changed file line',
      vim.log.levels.INFO
    )
    return
  end

  local revision = parse.find_revision_for_line(lines, cursor_line)
  require('badjuju').execute(server_command, { file, revision })
end

local LOG_MAPS = {
  { 'g', 'JJRefresh', 'badjuju: refresh' },
  { 'R', 'JJRefresh', 'badjuju: refresh' },
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

--- Show a floating help popup listing active bindings for window_type.
--- Fetches entries from the server's badjuju.help command.
---@param window_type string  'status' | 'log' | 'diff'
local function show_help(window_type)
  require('badjuju').request('badjuju.help', { window_type }, function(entries)
      if type(entries) ~= 'table' or #entries == 0 then
        vim.notify('badjuju: no keymap entries for ' .. window_type, vim.log.levels.INFO)
        return
      end

      -- Build lines: "key   description"
      local max_key = 0
      for _, e in ipairs(entries) do
        if type(e.key) == 'string' and #e.key > max_key then
          max_key = #e.key
        end
      end
      local lines = { ' Bad Juju — ' .. window_type .. ' bindings', '' }
      for _, e in ipairs(entries) do
        if type(e.key) == 'string' and e.key ~= '' then
          local pad = string.rep(' ', max_key - #e.key + 3)
          lines[#lines + 1] = ' ' .. e.key .. pad .. (e.description or '')
        end
      end
      lines[#lines + 1] = ''

      local width = 0
      for _, l in ipairs(lines) do
        if #l > width then width = #l end
      end
      width = math.max(width, 30)

      local buf = vim.api.nvim_create_buf(false, true)
      vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
      vim.bo[buf].modifiable = false

      local win_height = vim.api.nvim_list_uis()[1] and vim.api.nvim_list_uis()[1].height or 24
      local win_width  = vim.api.nvim_list_uis()[1] and vim.api.nvim_list_uis()[1].width  or 80
      local row = math.floor((win_height - #lines) / 2)
      local col = math.floor((win_width  - width)  / 2)

      local win = vim.api.nvim_open_win(buf, true, {
        relative = 'editor',
        row      = row,
        col      = col,
        width    = width,
        height   = #lines,
        style    = 'minimal',
        border   = 'rounded',
        title    = ' ? Help ',
        title_pos = 'center',
      })
      vim.wo[win].cursorline = false

      -- Close on q, Escape, or ?
      for _, key in ipairs({ 'q', '<Esc>', '?' }) do
        vim.keymap.set('n', key, function()
          if vim.api.nvim_win_is_valid(win) then
            vim.api.nvim_win_close(win, true)
          end
        end, { buffer = buf, silent = true, nowait = true })
      end
  end)
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
    nmap(bufnr, 'd', function() run_at_cursor_split('status', 'badjuju.describe', 'describe') end,
      'badjuju: describe commit at cursor in a split')
    nmap(bufnr, 'D', function() run_at_cursor_split('status', 'badjuju.diff', 'diff') end,
      'badjuju: diff commit at cursor in a split')
    nmap(bufnr, 's', function() run_file_scoped('badjuju.squash') end,
      'badjuju: squash file at cursor into parent')
    nmap(bufnr, 'U', function() run_file_scoped('badjuju.unsquash') end,
      'badjuju: unsquash file at cursor from parent into child')
    nmap(bufnr, '?', function() show_help('status') end, 'badjuju: show help')
  elseif name:match('/%.jj/badjuju/log%.jujutsu$') then
    for _, m in ipairs(LOG_MAPS) do
      map_cmd(bufnr, m[1], m[2], m[3])
    end
    nmap(bufnr, 'q', '<Cmd>quit<CR>', 'badjuju: close window')
    nmap(bufnr, 'd', function() run_at_cursor_split('log', 'badjuju.describe', 'describe') end,
      'badjuju: describe commit at cursor in a split')
    nmap(bufnr, 'D', function() run_at_cursor_split('log', 'badjuju.diff', 'diff') end,
      'badjuju: diff commit at cursor in a split')
    nmap(bufnr, '<CR>', apply_log_shortcut, 'badjuju: apply revset shortcut under cursor')
    nmap(bufnr, '?', function() show_help('log') end, 'badjuju: show help')
  elseif name:match('/%.jj/badjuju/diff%.jujutsu$') then
    nmap(bufnr, 'g', '<Cmd>JJRefresh<CR>', 'badjuju: refresh')
    nmap(bufnr, 'R', '<Cmd>JJRefresh<CR>', 'badjuju: refresh')
    nmap(bufnr, 'q', '<Cmd>quit<CR>', 'badjuju: close window')
    nmap(bufnr, '?', function() show_help('diff') end, 'badjuju: show help')
  elseif name:match('/%.jj/badjuju/describe%.jujutsu$') then
    nmap(bufnr, '?', function() show_help('describe') end, 'badjuju: show help')
    nmap(bufnr, '<C-c><C-c>', '<Cmd>write | quit<CR>', 'badjuju: finalize commit (save and close)')
    nmap(bufnr, '<C-c><C-k>', '<Cmd>quit!<CR>', 'badjuju: abort (close without saving)')
    vim.keymap.set('i', '<C-c>', '<Esc><Cmd>write | quit<CR>', {
      buffer = bufnr,
      silent = true,
      nowait = true,
      desc = 'badjuju: finalize commit (save and close)',
    })
  end
end

return M
