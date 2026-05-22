local M = {}

--- Plugin configuration. Mutate via setup(); fields are nil until populated.
---@class badjuju.Config
---@field binary_path string?        Path to the jj binary (forwarded to the server).
---@field default_log_revset string? Default revset used by :JJLog with no argument.
---@field keymap_profile string?     Keymap profile: "magit" (default) or "none" (no bindings).
M.config = {
  binary_path = nil,
  default_log_revset = nil,
  keymap_profile = nil,
}

local CLIENT_NAME = 'jujutsu'

--- Populate the plugin config. Safe to call before vim.lsp.enable('jujutsu')
--- because lsp/jujutsu.lua reads M.config when the LSP starts.
---
--- Accepts both the camelCase keys exposed in the VS Code settings
--- (binaryPath, defaultLogRevset) and the snake_case internal keys.
---@param opts table?
function M.setup(opts)
  opts = opts or {}
  if opts.binaryPath ~= nil then
    M.config.binary_path = opts.binaryPath
  end
  if opts.binary_path ~= nil then
    M.config.binary_path = opts.binary_path
  end
  if opts.defaultLogRevset ~= nil then
    M.config.default_log_revset = opts.defaultLogRevset
  end
  if opts.default_log_revset ~= nil then
    M.config.default_log_revset = opts.default_log_revset
  end
  if opts.keymapProfile ~= nil then
    M.config.keymap_profile = opts.keymapProfile
  end
  if opts.keymap_profile ~= nil then
    M.config.keymap_profile = opts.keymap_profile
  end
end

function M.get_client()
  local clients = vim.lsp.get_clients({ name = CLIENT_NAME })
  return clients[1]
end

--- Locate the .jj workspace root from the current buffer's path or cwd.
--- Returns nil when neither is inside a jj workspace.
---@return string?
function M.find_workspace_root()
  local source = vim.api.nvim_buf_get_name(0)
  if source == '' then
    source = vim.fn.getcwd()
  end
  return vim.fs.root(source, '.jj')
end

--- Maximum time (ms) M.ensure_client() will block waiting for a freshly-started
--- LSP client to finish its initialize handshake.
local INITIALIZE_TIMEOUT_MS = 2000

--- Start the jujutsu LSP client for the current workspace, without attaching
--- to any buffer. Returns the client (existing or freshly started), or nil if
--- the current location isn't inside a jj workspace.
---
--- Blocks until the client finishes the LSP initialize handshake. vim.lsp.start
--- returns as soon as the child process is spawned, but tower-lsp rejects
--- workspace/executeCommand requests sent before the initialized notification
--- with "Server not initialized" — which is what the first :JJStatus call
--- after a fresh start would otherwise hit. vim.wait pumps the event loop so
--- the response message can actually be processed.
---@return vim.lsp.Client?
function M.ensure_client()
  local existing = M.get_client()
  if existing then
    return existing
  end

  local root = M.find_workspace_root()
  if not root then
    return nil
  end

  local init_options
  if M.config.binary_path and M.config.binary_path ~= '' then
    init_options = { binaryPath = M.config.binary_path }
  end

  local client_id = vim.lsp.start({
    name = CLIENT_NAME,
    cmd = { 'badjuju', 'lsp' },
    root_dir = root,
    init_options = init_options,
  }, { attach = false })

  if not client_id then
    return nil
  end

  local client = vim.lsp.get_client_by_id(client_id)
  if client and not client.initialized then
    vim.wait(INITIALIZE_TIMEOUT_MS, function()
      return client.initialized == true
    end, 10)
    if not client.initialized then
      vim.notify(
        'badjuju: LSP server failed to initialize within '
          .. INITIALIZE_TIMEOUT_MS
          .. 'ms.',
        vim.log.levels.ERROR
      )
      return nil
    end
  end
  return client
end

--- Send workspace/executeCommand to the jujutsu LSP and open the returned file URI.
--- Starts the LSP client on demand if none is attached, so :JJ* commands work
--- from any buffer inside a jj workspace (not only after opening a .jujutsu file).
---@param command string  badjuju.* server command name
---@param arguments any[]?  optional arguments forwarded to the server
---@param opts { after?: fun(result_uri: string), split?: 'h'|'v' }?
---   after: optional callback invoked on the main loop after the result file
---     has been opened and reloaded. Receives the server-returned URI string.
---   split: open the result file in a new horizontal ('h') or vertical ('v')
---     split window instead of replacing the current window's buffer.
function M.execute(command, arguments, opts)
  local client = M.ensure_client()
  if not client then
    vim.notify(
      'badjuju: not in a jj workspace (no .jj directory found from current buffer or cwd).',
      vim.log.levels.ERROR
    )
    return
  end

  client:request('workspace/executeCommand', {
    command = command,
    arguments = arguments or {},
  }, function(err, result)
    if err then
      vim.notify('badjuju: ' .. (err.message or vim.inspect(err)), vim.log.levels.ERROR)
      return
    end
    if type(result) ~= 'string' or result == '' then
      return
    end
    vim.schedule(function()
      if opts and opts.split == 'h' then
        vim.cmd('split')
      elseif opts and opts.split == 'v' then
        vim.cmd('vsplit')
      end
      vim.lsp.util.show_document(
        { uri = result },
        client.offset_encoding,
        { focus = true }
      )
      -- The server just wrote this file; force the focused buffer to reload
      -- so a previously-open status.jujutsu/log.jujutsu reflects the new
      -- content instead of showing stale text. Skip describe.jujutsu so an
      -- in-progress commit message isn't discarded.
      local fname = vim.uri_to_fname(result)
      if not fname:match('/describe%.jujutsu$') then
        pcall(vim.cmd, 'silent! edit!')
      end
      if opts and opts.after then
        opts.after(result)
      end
    end)
  end)
end

--- Send workspace/executeCommand and pass the raw result to cb.
--- Unlike execute(), no file-opening is done — use this for commands that
--- return structured data (e.g. badjuju.help, badjuju.keymap).
---@param command string
---@param arguments any[]?
---@param cb fun(result: any)
function M.request(command, arguments, cb)
  local client = M.ensure_client()
  if not client then
    vim.notify(
      'badjuju: not in a jj workspace (no .jj directory found).',
      vim.log.levels.ERROR
    )
    return
  end
  client:request('workspace/executeCommand', {
    command = command,
    arguments = arguments or {},
  }, function(err, result)
    if err then
      vim.notify('badjuju: ' .. (err.message or vim.inspect(err)), vim.log.levels.ERROR)
      return
    end
    vim.schedule(function() cb(result) end)
  end)
end

return M
