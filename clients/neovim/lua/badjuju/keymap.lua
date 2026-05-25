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
  { 'R', 'JJRefresh',    'badjuju: refresh' },
  { 'n', 'JJNew',        'badjuju: new change' },
  { 'L', 'JJLog',        'badjuju: open log' },
  { 'f', 'JJFetch',      'badjuju: git fetch' },
  { 'p', 'JJPush',       'badjuju: git push' },
  { 'u', 'JJUndo',       'badjuju: undo' },
  { '=', 'JJToggleStat', 'badjuju: toggle --stat' },
  { 'a', 'JJAbandon',    'badjuju: abandon revision' },
}

--- Build a `{cursor = {uri, line}}` argument the server can resolve to a
--- revision (or file) on the current jujutsu buffer. Read from the current
--- window's cursor.
---@return table
local function cursor_arg()
  local name = vim.api.nvim_buf_get_name(0)
  local cursor_line = vim.api.nvim_win_get_cursor(0)[1] - 1 -- 0-indexed
  return { cursor = { uri = vim.uri_from_fname(name), line = cursor_line } }
end

--- Run a server command for the commit under the cursor, opening the result
--- in a horizontal split. The cursor position is shipped to the server which
--- resolves the revision; a server-side "no revision at cursor" error surfaces
--- through the normal LSP error path.
---@param server_command string  badjuju.* command name
local function run_at_cursor_split(server_command)
  require('badjuju').execute(server_command, { cursor_arg() }, { split = 'h' })
end

--- Run a file-scoped status command. The cursor position is shipped to the
--- server, which resolves both the file and the revision from the same line
--- of status.jujutsu. A server-side "no file at cursor" error surfaces through
--- the normal LSP error path.
---@param server_command 'badjuju.squash'|'badjuju.unsquash'
local function run_file_scoped(server_command)
  require('badjuju').execute(server_command, { cursor_arg() })
end

local LOG_MAPS = {
  { 'R', 'JJRefresh', 'badjuju: refresh' },
  { 'a', 'JJAbandon', 'badjuju: abandon revision' },
}

--- Handle <CR> on a log.jujutsu buffer. Ships the cursor position to the
--- server which re-runs `badjuju.log` with the revset of any `JJ: <label>:
--- <revset>` shortcut at that line. The cursor is restored (row/col clamped
--- to the new buffer) after the buffer regenerates. If the cursor isn't on
--- a shortcut line the server returns an LSP error which surfaces through
--- the standard notification path.
local function apply_log_shortcut()
  local win = vim.api.nvim_get_current_win()
  local saved = vim.api.nvim_win_get_cursor(win) -- {row (1-based), col}

  require('badjuju').execute('badjuju.log', { cursor_arg() }, {
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
--- When keymapProfile is 'none', skips all hotkey registrations (users bind their own).
---@param bufnr integer?  defaults to current buffer (0)
function M.setup_for_buffer(bufnr)
  bufnr = bufnr or 0
  local profile = require('badjuju').config.keymap_profile
  if profile == 'none' then
    return
  end
  local name = vim.api.nvim_buf_get_name(bufnr)

  if name:match('/%.jj/badjuju/status%.jujutsu$') then
    -- Shared helpers used by both profiles. Both send a cursor-form arg so
    -- the server resolves the revision; create/move on a non-commit line
    -- surfaces a server-side error rather than the old client-side check.
    local function status_bookmark()
      local arg = cursor_arg()
      local ACTIONS = { 'create', 'move', 'delete', 'track', 'forget' }
      vim.ui.select(ACTIONS, { prompt = 'jj bookmark: ' }, function(sub_action)
        if not sub_action then return end
        local prompt = sub_action == 'track' and 'Bookmark (e.g. main@origin): ' or 'Bookmark name: '
        vim.ui.input({ prompt = prompt }, function(bname)
          if not bname or bname == '' then return end
          local rev = (sub_action == 'create' or sub_action == 'move') and arg or ''
          require('badjuju').execute('badjuju.bookmark', { sub_action, bname, rev })
        end)
      end)
    end
    local function status_rebase()
      local arg = cursor_arg()
      vim.ui.input({ prompt = 'Rebase to: ' }, function(dest)
        if not dest or dest == '' then return end
        require('badjuju').execute('badjuju.rebase', { arg, dest })
      end)
    end

    if profile == 'vim' then
      nmap(bufnr, 'nn', '<Cmd>JJNew<CR>', 'badjuju: new change')
      nmap(bufnr, 'll', '<Cmd>JJLog<CR>', 'badjuju: open log')
      nmap(bufnr, 'ss', function() run_file_scoped('badjuju.squash') end,
        'badjuju: squash file at cursor into parent')
      nmap(bufnr, 'UU', function() run_file_scoped('badjuju.unsquash') end,
        'badjuju: unsquash file at cursor from parent into child')
      nmap(bufnr, 'ff', '<Cmd>JJFetch<CR>', 'badjuju: git fetch')
      nmap(bufnr, 'pp', '<Cmd>JJPush<CR>', 'badjuju: git push')
      nmap(bufnr, 'PP', '<Cmd>JJPush!<CR>', 'badjuju: git push --force-with-lease')
      nmap(bufnr, 'uu', '<Cmd>JJUndo<CR>', 'badjuju: undo')
      nmap(bufnr, '=', '<Cmd>JJToggleStat<CR>', 'badjuju: toggle --stat')
      nmap(bufnr, 'bb', status_bookmark, 'badjuju: bookmark (create / move / delete / track / forget)')
      nmap(bufnr, 'rr', status_rebase, 'badjuju: rebase commit at cursor to destination')
      nmap(bufnr, 'ee', function() run_at_cursor_split('badjuju.edit') end,
        'badjuju: edit commit at cursor (move @)')
      nmap(bufnr, 'dd', function() run_at_cursor_split('badjuju.describe') end,
        'badjuju: describe commit at cursor in a split')
      nmap(bufnr, 'D', function() run_at_cursor_split('badjuju.diff') end,
        'badjuju: diff commit at cursor in a split')
      nmap(bufnr, 'aa', '<Cmd>JJAbandon<CR>', 'badjuju: abandon revision')
    else
      for _, m in ipairs(STATUS_MAPS) do
        map_cmd(bufnr, m[1], m[2], m[3])
      end
      nmap(bufnr, 'P', '<Cmd>JJPush!<CR>', 'badjuju: git push --force-with-lease')
      nmap(bufnr, 'e', function() run_at_cursor_split('badjuju.edit') end,
        'badjuju: edit commit at cursor (move @)')
      nmap(bufnr, 'b', status_bookmark, 'badjuju: bookmark (create / move / delete / track / forget)')
      nmap(bufnr, 'r', status_rebase, 'badjuju: rebase commit at cursor to destination')
      nmap(bufnr, 'd', function() run_at_cursor_split('badjuju.describe') end,
        'badjuju: describe commit at cursor in a split')
      nmap(bufnr, 'D', function() run_at_cursor_split('badjuju.diff') end,
        'badjuju: diff commit at cursor in a split')
      nmap(bufnr, 's', function() run_file_scoped('badjuju.squash') end,
        'badjuju: squash file at cursor into parent')
      nmap(bufnr, 'U', function() run_file_scoped('badjuju.unsquash') end,
        'badjuju: unsquash file at cursor from parent into child')
    end
    nmap(bufnr, 'q', '<Cmd>quit<CR>', 'badjuju: close window')
    nmap(bufnr, '?', function() show_help('status') end, 'badjuju: show help')
    -- Open the LSP code-action menu for the current cursor line. Server emits
    -- actions for commit headers (edit/abandon/describe/diff/new/rebase/bookmark)
    -- and file lines (squash/unsquash).
    if profile == 'vim' then
      nmap(bufnr, '<leader>a', vim.lsp.buf.code_action, 'badjuju: code actions menu')
    else
      nmap(bufnr, 'A', vim.lsp.buf.code_action, 'badjuju: code actions menu')
    end
  elseif name:match('/%.jj/badjuju/log%.jujutsu$') then
    -- Both helpers send cursor-form args; the server returns an LSP error if
    -- the cursor isn't on a commit (surfaced via badjuju.execute's error path).
    local function log_bookmark()
      local arg = cursor_arg()
      local ACTIONS = { 'create', 'move', 'delete', 'track', 'forget' }
      vim.ui.select(ACTIONS, { prompt = 'jj bookmark: ' }, function(sub_action)
        if not sub_action then return end
        local prompt = sub_action == 'track' and 'Bookmark (e.g. main@origin): ' or 'Bookmark name: '
        vim.ui.input({ prompt = prompt }, function(bname)
          if not bname or bname == '' then return end
          local rev = (sub_action == 'create' or sub_action == 'move') and arg or ''
          require('badjuju').execute('badjuju.bookmark', { sub_action, bname, rev })
        end)
      end)
    end
    local function log_rebase()
      local arg = cursor_arg()
      vim.ui.input({ prompt = 'Rebase to: ' }, function(dest)
        if not dest or dest == '' then return end
        require('badjuju').execute('badjuju.rebase', { arg, dest })
      end)
    end

    if profile == 'vim' then
      nmap(bufnr, 'bb', log_bookmark, 'badjuju: bookmark (create / move / delete / track / forget)')
      nmap(bufnr, 'rr', log_rebase, 'badjuju: rebase commit at cursor to destination')
      nmap(bufnr, 'ee', function() run_at_cursor_split('badjuju.edit') end,
        'badjuju: edit commit at cursor (move @)')
      nmap(bufnr, 'dd', function() run_at_cursor_split('badjuju.describe') end,
        'badjuju: describe commit at cursor in a split')
      nmap(bufnr, 'D', function() run_at_cursor_split('badjuju.diff') end,
        'badjuju: diff commit at cursor in a split')
      nmap(bufnr, 'aa', '<Cmd>JJAbandon<CR>', 'badjuju: abandon revision')
      for _, m in ipairs(LOG_MAPS) do
        if m[1] ~= 'a' then map_cmd(bufnr, m[1], m[2], m[3]) end
      end
    else
      for _, m in ipairs(LOG_MAPS) do
        map_cmd(bufnr, m[1], m[2], m[3])
      end
      nmap(bufnr, 'e', function() run_at_cursor_split('badjuju.edit') end,
        'badjuju: edit commit at cursor (move @)')
      nmap(bufnr, 'b', log_bookmark, 'badjuju: bookmark (create / move / delete / track / forget)')
      nmap(bufnr, 'r', log_rebase, 'badjuju: rebase commit at cursor to destination')
      nmap(bufnr, 'd', function() run_at_cursor_split('badjuju.describe') end,
        'badjuju: describe commit at cursor in a split')
      nmap(bufnr, 'D', function() run_at_cursor_split('badjuju.diff') end,
        'badjuju: diff commit at cursor in a split')
    end
    nmap(bufnr, '<CR>', apply_log_shortcut, 'badjuju: apply revset shortcut under cursor')
    nmap(bufnr, '?', function() show_help('log') end, 'badjuju: show help')
    if profile == 'vim' then
      nmap(bufnr, '<leader>a', vim.lsp.buf.code_action, 'badjuju: code actions menu')
    else
      nmap(bufnr, 'A', vim.lsp.buf.code_action, 'badjuju: code actions menu')
    end
  elseif name:match('/%.jj/badjuju/diff%.jujutsu$') then
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
