describe('status.jujutsu s/U buffer maps', function()
  local keymap = require('badjuju.keymap')

  local counter = 0
  local function unique(rel)
    counter = counter + 1
    return vim.fn.tempname() .. '-fs' .. counter .. '/' .. rel
  end

  local function has_buffer_map(bufnr, key)
    for _, m in ipairs(vim.api.nvim_buf_get_keymap(bufnr, 'n')) do
      if m.lhs == key then return true end
    end
    return false
  end

  it('binds s and U on status.jujutsu', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique('.jj/badjuju/status.jujutsu'))
    keymap.setup_for_buffer(0)
    local buf = vim.api.nvim_get_current_buf()
    assert.is_true(has_buffer_map(buf, 's'), 'expected s map on status.jujutsu')
    assert.is_true(has_buffer_map(buf, 'U'), 'expected U map on status.jujutsu')
  end)

  it('binds s and U on log.jujutsu', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique('.jj/badjuju/log.jujutsu'))
    keymap.setup_for_buffer(0)
    local buf = vim.api.nvim_get_current_buf()
    assert.is_true(has_buffer_map(buf, 's'), 'log.jujutsu should bind s (squash.commit)')
    assert.is_true(has_buffer_map(buf, 'U'), 'log.jujutsu should bind U (undo)')
  end)
end)
