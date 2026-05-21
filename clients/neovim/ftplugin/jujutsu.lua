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

require('badjuju.keymap').setup_for_buffer(0)
