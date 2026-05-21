local badjuju = require('badjuju')

-- Reset module state between tests since the module is cached across them.
local function reset()
  badjuju.config.binary_path = nil
  badjuju.config.default_log_revset = nil
end

describe('badjuju.setup', function()
  before_each(reset)

  it('is a no-op when called with no args', function()
    badjuju.setup()
    assert.is_nil(badjuju.config.binary_path)
    assert.is_nil(badjuju.config.default_log_revset)
  end)

  it('accepts camelCase keys (matches VS Code settings)', function()
    badjuju.setup({ binaryPath = '/abs/jj', defaultLogRevset = 'trunk()..@' })
    assert.are.equal('/abs/jj', badjuju.config.binary_path)
    assert.are.equal('trunk()..@', badjuju.config.default_log_revset)
  end)

  it('accepts snake_case keys', function()
    badjuju.setup({ binary_path = '/snake', default_log_revset = 'x' })
    assert.are.equal('/snake', badjuju.config.binary_path)
    assert.are.equal('x', badjuju.config.default_log_revset)
  end)
end)

describe(':JJLog default revset round-trip', function()
  before_each(reset)

  -- Stub badjuju.execute so the command does not require a live LSP client.
  -- Returns the (command, arguments) tuple captured by the user command.
  local function capture_jjlog(...)
    local original = badjuju.execute
    local captured = {}
    badjuju.execute = function(cmd, args)
      captured.command = cmd
      captured.arguments = args
    end
    local ok, err = pcall(function(...)
      vim.cmd.JJLog(...)
    end, ...)
    badjuju.execute = original
    assert.is_true(ok, tostring(err))
    return captured
  end

  it('forwards the explicit revset', function()
    local got = capture_jjlog('@..main')
    assert.are.equal('badjuju.log', got.command)
    assert.are.same({ '@..main' }, got.arguments)
  end)

  it('uses the configured default when no arg is given', function()
    badjuju.setup({ defaultLogRevset = 'trunk()..@' })
    local got = capture_jjlog()
    assert.are.equal('badjuju.log', got.command)
    assert.are.same({ 'trunk()..@' }, got.arguments)
  end)

  it('sends no arguments when neither user nor config supplies a revset', function()
    local got = capture_jjlog()
    assert.are.same({}, got.arguments)
  end)
end)
