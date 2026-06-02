vim.bo.commentstring = 'JJ: %s'

-- Generated buffers the server rewrites on every command must not be edited.
-- log.jujutsu is intentionally NOT included: its REVSET: header is editable
-- and re-runs the query on save.
local name = vim.api.nvim_buf_get_name(0)
if
  name:match('/%.jj/badjuju/status%.jujutsu$')
  or name:match('/%.jj/badjuju/diff%.jujutsu$')
  or name:match('/%.jj/badjuju/diff%-[a-z]+%-[^/]+%.jujutsu$')
  or name:match('/%.jj/badjuju/squash/[^/]+%.jujutsu$')
then
  vim.bo.modifiable = false
  vim.bo.readonly = true
end

-- Reflect on-disk changes (the server rewrites these files behind our back).
vim.bo.autoread = true

-- After saving describe.jujutsu the server applies the description and
-- rewrites status.jujutsu / log.jujutsu on disk. Trigger :checktime so any
-- already-open status/log buffers reload (combined with autoread above)
-- instead of showing the stale pre-save description.
if name:match('/%.jj/badjuju/describe%.jujutsu$') then
  vim.api.nvim_create_autocmd('BufWritePost', {
    buffer = 0,
    callback = function()
      vim.defer_fn(function()
        pcall(vim.cmd, 'checktime')
      end, 250)
    end,
  })
end

-- Enable LSP-driven folding for status buffers and start fully folded.
if name:match('/%.jj/badjuju/status%.jujutsu$') then
  vim.wo.foldmethod = 'expr'
  vim.wo.foldexpr = 'v:lua.vim.lsp.foldexpr()'
  vim.wo.foldlevel = 0
  -- Show only the first line of each fold with no line-count prefix.
  _G._badjuju_foldtext = function()
    return vim.api.nvim_buf_get_lines(0, vim.v.foldstart - 1, vim.v.foldstart, false)[1]
  end
  vim.wo.foldtext = 'v:lua._badjuju_foldtext()'
  local buf = vim.api.nvim_get_current_buf()
  local win = vim.api.nvim_get_current_win()
  vim.defer_fn(function()
    if not vim.api.nvim_buf_is_valid(buf) or not vim.api.nvim_win_is_valid(win) then
      return
    end
    vim.api.nvim_win_call(win, function()
      pcall(vim.cmd, 'normal! zM')
      local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
      for i, line in ipairs(lines) do
        if line:match('^WORKING COPY CHANGES') or line:match('^PARENT CHANGES') then
          vim.api.nvim_win_set_cursor(win, {i, 0})
          pcall(vim.cmd, 'normal! zo')
        end
      end
      vim.api.nvim_win_set_cursor(win, {1, 0})
    end)
  end, 100)
end

-- Enable LSP-driven folding for squash buffers; start fully folded.
if name:match('/%.jj/badjuju/squash/[^/]+%.jujutsu$') then
  vim.wo.foldmethod = 'expr'
  vim.wo.foldexpr = 'v:lua.vim.lsp.foldexpr()'
  vim.wo.foldlevel = 0
  if _G._badjuju_foldtext == nil then
    _G._badjuju_foldtext = function()
      return vim.api.nvim_buf_get_lines(0, vim.v.foldstart - 1, vim.v.foldstart, false)[1]
    end
  end
  vim.wo.foldtext = 'v:lua._badjuju_foldtext()'
  local win = vim.api.nvim_get_current_win()
  vim.defer_fn(function()
    if vim.api.nvim_win_is_valid(win) then
      vim.api.nvim_win_call(win, function()
        pcall(vim.cmd, 'normal! zM')
      end)
    end
  end, 100)
end

require('badjuju.keymap').setup_for_buffer(0)
