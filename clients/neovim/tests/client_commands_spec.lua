local client_commands = require('badjuju.client_commands')

describe('client_commands.register', function()
  it('installs vim.lsp.commands entries for rebasePrompt and bookmarkPrompt', function()
    client_commands.register()
    assert.is_function(
      vim.lsp.commands['badjuju.client.rebasePrompt'],
      'expected badjuju.client.rebasePrompt handler'
    )
    assert.is_function(
      vim.lsp.commands['badjuju.client.bookmarkPrompt'],
      'expected badjuju.client.bookmarkPrompt handler'
    )
  end)

  it('rebasePrompt forwards revision + dest to badjuju.rebase', function()
    client_commands.register()
    local badjuju = require('badjuju')
    local captured = nil
    local original = badjuju.execute
    badjuju.execute = function(command, args)
      captured = { command = command, args = args }
    end
    local original_input = vim.ui.input
    vim.ui.input = function(_opts, cb) cb('main') end

    vim.lsp.commands['badjuju.client.rebasePrompt'](
      { command = 'badjuju.client.rebasePrompt', arguments = { 'abc12345' } },
      {}
    )

    badjuju.execute = original
    vim.ui.input = original_input

    assert.are.equal('badjuju.rebase', captured.command)
    assert.are.same({ 'abc12345', 'main' }, captured.args)
  end)

  it('rebasePrompt does nothing on empty dest', function()
    client_commands.register()
    local badjuju = require('badjuju')
    local called = false
    local original = badjuju.execute
    badjuju.execute = function() called = true end
    local original_input = vim.ui.input
    vim.ui.input = function(_opts, cb) cb('') end

    vim.lsp.commands['badjuju.client.rebasePrompt'](
      { command = 'badjuju.client.rebasePrompt', arguments = { 'abc12345' } },
      {}
    )

    badjuju.execute = original
    vim.ui.input = original_input
    assert.is_false(called, 'execute should not be called on empty dest')
  end)

  it('bookmarkPrompt with create sub_action forwards revision', function()
    client_commands.register()
    local badjuju = require('badjuju')
    local captured = nil
    local original = badjuju.execute
    badjuju.execute = function(command, args)
      captured = { command = command, args = args }
    end
    local original_select = vim.ui.select
    vim.ui.select = function(_choices, _opts, cb) cb('create') end
    local original_input = vim.ui.input
    vim.ui.input = function(_opts, cb) cb('feature') end

    vim.lsp.commands['badjuju.client.bookmarkPrompt'](
      { command = 'badjuju.client.bookmarkPrompt', arguments = { 'abc12345' } },
      {}
    )

    badjuju.execute = original
    vim.ui.select = original_select
    vim.ui.input = original_input

    assert.are.equal('badjuju.bookmark', captured.command)
    assert.are.same({ 'create', 'feature', 'abc12345' }, captured.args)
  end)

  it('bookmarkPrompt with delete sub_action sends empty revision', function()
    client_commands.register()
    local badjuju = require('badjuju')
    local captured = nil
    local original = badjuju.execute
    badjuju.execute = function(command, args)
      captured = { command = command, args = args }
    end
    local original_select = vim.ui.select
    vim.ui.select = function(_choices, _opts, cb) cb('delete') end
    local original_input = vim.ui.input
    vim.ui.input = function(_opts, cb) cb('feature') end

    vim.lsp.commands['badjuju.client.bookmarkPrompt'](
      { command = 'badjuju.client.bookmarkPrompt', arguments = { 'abc12345' } },
      {}
    )

    badjuju.execute = original
    vim.ui.select = original_select
    vim.ui.input = original_input

    -- delete/track/forget don't need the revision; passed as "".
    assert.are.same({ 'delete', 'feature', '' }, captured.args)
  end)
end)
