vim.bo.commentstring = 'JJ: %s'

-- Generated buffers the server rewrites on every command must not be edited.
-- log.jujutsu is intentionally NOT included: its REVSET: header is editable
-- and re-runs the query on save.
local name = vim.api.nvim_buf_get_name(0)
if
  name:match('/%.jj/badjuju/status%.jujutsu$')
  or name:match('/%.jj/badjuju/diff%.jujutsu$')
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

require('badjuju.keymap').setup_for_buffer(0)
