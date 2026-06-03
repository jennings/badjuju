local keymap = require('badjuju.keymap')

local counter = 0
local function unique_path(relative)
  counter = counter + 1
  return vim.fn.tempname() .. '-keymap' .. counter .. '/' .. relative
end

local function open_named(relative)
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, unique_path(relative))
  keymap.setup_for_buffer(0)
  return vim.api.nvim_get_current_buf()
end

local function has_buffer_map(bufnr, key)
  for _, m in ipairs(vim.api.nvim_buf_get_keymap(bufnr, 'n')) do
    if m.lhs == key then
      return true, m
    end
  end
  return false, nil
end

describe('keymap.setup_for_buffer', function()
  it('installs status.jujutsu maps including R/r/n/L/d/q/u/= and a', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    for _, key in ipairs({ 'R', 'r', 'n', 'L', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected status.jujutsu map for ' .. key)
    end
  end)

  it('does NOT install g on status.jujutsu (would shadow gra code-action sequence)', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found = has_buffer_map(buf, 'g')
    assert.is_false(found, 'status.jujutsu should not bind g')
  end)

  it('installs log.jujutsu maps for refresh, abandon, describe, and diff', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    for _, key in ipairs({ 'R', 'r', 'a', 'd', 'D' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected log.jujutsu map for ' .. key)
    end
  end)

  it('does NOT install g on log.jujutsu (would shadow gra code-action sequence)', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    local found = has_buffer_map(buf, 'g')
    assert.is_false(found, 'log.jujutsu should not bind g')
  end)

  it('installs D for diff on status.jujutsu', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found = has_buffer_map(buf, 'D')
    assert.is_true(found, 'expected status.jujutsu map for D')
  end)

  it('installs diff.jujutsu maps for refresh and close', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    for _, key in ipairs({ 'R', 'q' }) do
      local found = has_buffer_map(buf, key)
      assert.is_true(found, 'expected diff.jujutsu map for ' .. key)
    end
  end)

  it('does NOT install g or r on diff.jujutsu (would shadow gra code-action sequence)', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    for _, key in ipairs({ 'g', 'r' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'diff.jujutsu should not bind ' .. key)
    end
  end)

  it('does NOT install commit-action maps on diff.jujutsu', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    for _, key in ipairs({ 'n', 'L', 'd', 'D', 'a', '=', 'u', 's', 'U' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'diff.jujutsu should not bind ' .. key)
    end
  end)

  it('installs <CR> on log.jujutsu for revset shortcut application', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    local found = has_buffer_map(buf, '<CR>')
    assert.is_true(found, 'expected log.jujutsu map for <CR>')
  end)

  it('does NOT install <CR> on status.jujutsu', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found = has_buffer_map(buf, '<CR>')
    assert.is_false(found, 'status.jujutsu should not bind <CR>')
  end)

  it('does NOT install status-only maps on log.jujutsu', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    for _, key in ipairs({ 'n', 'L', 'q', '=', 'u' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'log.jujutsu should not bind ' .. key)
    end
  end)

  it('installs code-action binding A on status.jujutsu (magit profile)', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    assert.is_true(has_buffer_map(buf, 'A'), 'expected A map on status.jujutsu')
  end)

  it('installs code-action binding A on log.jujutsu (magit profile)', function()
    local buf = open_named('.jj/badjuju/log.jujutsu')
    assert.is_true(has_buffer_map(buf, 'A'), 'expected A map on log.jujutsu')
  end)

  it('q on status.jujutsu maps to bdelete (closes buffer, not editor)', function()
    local buf = open_named('.jj/badjuju/status.jujutsu')
    local found, m = has_buffer_map(buf, 'q')
    assert.is_true(found, 'expected status.jujutsu map for q')
    assert.is_truthy(
      m.rhs and m.rhs:lower():match('bdelete'),
      'q should invoke bdelete, got rhs=' .. tostring(m.rhs)
    )
    assert.is_falsy(
      m.rhs and m.rhs:lower():match('<cmd>quit'),
      'q must not invoke quit (would exit editor when last window)'
    )
  end)

  it('q on diff.jujutsu maps to bdelete', function()
    local buf = open_named('.jj/badjuju/diff.jujutsu')
    local found, m = has_buffer_map(buf, 'q')
    assert.is_true(found, 'expected diff.jujutsu map for q')
    assert.is_truthy(m.rhs and m.rhs:lower():match('bdelete'), 'q should invoke bdelete on diff')
  end)

  it('q on diff-change-<id>.jujutsu maps to bdelete', function()
    local buf = open_named('.jj/badjuju/diff-change-abcdef.jujutsu')
    local found, m = has_buffer_map(buf, 'q')
    assert.is_true(found, 'expected diff-change-*.jujutsu map for q')
    assert.is_truthy(m.rhs and m.rhs:lower():match('bdelete'), 'q should invoke bdelete on change diff')
  end)

  it('q on squash window maps to bdelete', function()
    local buf = open_named('.jj/badjuju/squash/foo-bar.jujutsu')
    local found, m = has_buffer_map(buf, 'q')
    assert.is_true(found, 'expected squash window map for q')
    assert.is_truthy(m.rhs and m.rhs:lower():match('bdelete'), 'q should invoke bdelete on squash')
  end)

  it('<Tab> on squash window maps to za (fold toggle)', function()
    local buf = open_named('.jj/badjuju/squash/foo-bar.jujutsu')
    local found, m = has_buffer_map(buf, '<Tab>')
    assert.is_true(found, 'expected squash window map for <Tab>')
    assert.are.equal('za', m.rhs, '<Tab> should invoke za (toggle fold)')
  end)

  it('does nothing for unrelated jujutsu buffers', function()
    local buf = open_named('describe.jujutsu')
    for _, key in ipairs({ 'R', 'r', 'n', 'L', 'd', 'q', 'u', '=', 'a' }) do
      local found = has_buffer_map(buf, key)
      assert.is_false(found, 'unrelated buffer should not bind ' .. key)
    end
  end)
end)

describe('revision-scoped hotkeys send cursor-form', function()
  local badjuju = require('badjuju')

  local function capture_execute()
    local captured = {}
    local original = badjuju.execute
    badjuju.execute = function(command, args, opts)
      captured[#captured + 1] = {
        command = command,
        args = args,
        opts = opts,
      }
    end
    return captured, function()
      badjuju.execute = original
    end
  end

  local function setup_buffer(relative, content)
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique_path(relative))
    vim.api.nvim_buf_set_lines(0, 0, -1, false, content)
    require('badjuju.keymap').setup_for_buffer(0)
    return vim.api.nvim_get_current_buf()
  end

  local function find_callback(bufnr, key)
    for _, m in ipairs(vim.api.nvim_buf_get_keymap(bufnr, 'n')) do
      if m.lhs == key and m.callback then
        return m.callback
      end
    end
    return nil
  end

  for _, case in ipairs({
    { key = 'e', server_command = 'badjuju.edit' },
    { key = 'd', server_command = 'badjuju.describe' },
    { key = 'D', server_command = 'badjuju.diff' },
  }) do
    it(
      'log.jujutsu ' .. case.key .. ' sends cursor-form for ' .. case.server_command,
      function()
        local captured, restore = capture_execute()
        local buf = setup_buffer('.jj/badjuju/log.jujutsu', {
          'REVSET: @',
          '',
          'OUTPUT:',
          '',
          '@  qpvuntsm 1234abcd',
          '│  description',
        })
        -- Cursor on the @ commit header (line 5, 1-indexed).
        vim.api.nvim_win_set_cursor(0, { 5, 0 })

        local cb = find_callback(buf, case.key)
        assert.is_not_nil(cb, 'expected callback for ' .. case.key)
        cb()
        restore()

        assert.are.equal(1, #captured, 'expected exactly one execute call')
        assert.are.equal(case.server_command, captured[1].command)
        assert.are.equal(1, #captured[1].args, 'expected one argument')
        local arg = captured[1].args[1]
        assert.is_table(arg.cursor, 'arg.cursor should be a table')
        assert.are.equal(4, arg.cursor.line, 'cursor line is 0-indexed (row 5 -> 4)')
        assert.is_truthy(
          arg.cursor.uri:match('/log%.jujutsu$'),
          'cursor uri should end in /log.jujutsu, got ' .. tostring(arg.cursor.uri)
        )
      end
    )
  end

  for _, case in ipairs({
    { key = 's', server_command = 'badjuju.squash.commit' },
    { key = 'U', server_command = 'badjuju.unsquash' },
  }) do
    it(
      'status.jujutsu ' .. case.key .. ' sends cursor-form for ' .. case.server_command,
      function()
        local captured, restore = capture_execute()
        local buf = setup_buffer('.jj/badjuju/status.jujutsu', {
          'STATUS:',
          '',
          'Working copy changes:',
          'M src/main.rs',
        })
        -- Cursor on the M file line (line 4, 1-indexed).
        vim.api.nvim_win_set_cursor(0, { 4, 0 })

        local cb = find_callback(buf, case.key)
        assert.is_not_nil(cb, 'expected callback for ' .. case.key)
        cb()
        restore()

        assert.are.equal(1, #captured)
        assert.are.equal(case.server_command, captured[1].command)
        local arg = captured[1].args[1]
        assert.are.equal(3, arg.cursor.line, 'cursor line is 0-indexed (row 4 -> 3)')
        assert.is_truthy(arg.cursor.uri:match('/status%.jujutsu$'))
      end
    )
  end

  it('log.jujutsu <CR> sends cursor-form for badjuju.log', function()
    local captured, restore = capture_execute()
    local buf = setup_buffer('.jj/badjuju/log.jujutsu', {
      'REVSET: @',
      'JJ: Mutable: ancestors(reachable(@, mutable()))',
      '',
      '@  qpvuntsm 1234abcd',
    })
    -- Cursor on the JJ: shortcut line (line 2, 1-indexed).
    vim.api.nvim_win_set_cursor(0, { 2, 0 })

    local cb = find_callback(buf, '<CR>')
    assert.is_not_nil(cb, 'expected callback for <CR>')
    cb()
    restore()

    assert.are.equal(1, #captured)
    assert.are.equal('badjuju.log', captured[1].command)
    local arg = captured[1].args[1]
    assert.are.equal(1, arg.cursor.line, 'cursor line is 0-indexed (row 2 -> 1)')
    assert.is_truthy(arg.cursor.uri:match('/log%.jujutsu$'))
  end)

  it('status.jujutsu e sends cursor-form for badjuju.edit', function()
    local captured, restore = capture_execute()
    local buf = setup_buffer('.jj/badjuju/status.jujutsu', {
      'STATUS:',
      '',
      'Working copy changes:',
      'M src/main.rs',
      '',
      'STACK:',
      '',
      '@  qpvuntsm 1234abcd',
    })
    -- Cursor on the @ commit header (line 8, 1-indexed).
    vim.api.nvim_win_set_cursor(0, { 8, 0 })

    local cb = find_callback(buf, 'e')
    assert.is_not_nil(cb, 'expected callback for e')
    cb()
    restore()

    assert.are.equal(1, #captured)
    assert.are.equal('badjuju.edit', captured[1].command)
    local arg = captured[1].args[1]
    assert.are.equal(7, arg.cursor.line)
    assert.is_truthy(arg.cursor.uri:match('/status%.jujutsu$'))
  end)
end)
