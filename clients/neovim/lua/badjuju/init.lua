local M = {}

--- Plugin configuration. Mutate via setup(); fields are nil until populated.
---@class badjuju.Config
---@field binary_path string?       Path to the jj binary (forwarded to the server).
---@field default_log_revset string?  Default revset used by :JJLog with no argument.
M.config = {
  binary_path = nil,
  default_log_revset = nil,
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
end

function M.get_client()
  local clients = vim.lsp.get_clients({ name = CLIENT_NAME })
  return clients[1]
end

--- Send workspace/executeCommand to the jujutsu LSP and open the returned file URI.
---@param command string  badjuju.* server command name
---@param arguments any[]?  optional arguments forwarded to the server
function M.execute(command, arguments)
  local client = M.get_client()
  if not client then
    vim.notify(
      'badjuju: no jujutsu LSP client attached. Open a .jj file in a jj workspace first.',
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
      vim.lsp.util.show_document(
        { uri = result },
        client.offset_encoding,
        { focus = true }
      )
      -- The server just wrote this file; force the focused buffer to reload
      -- so a previously-open status.jj/log.jj reflects the new content
      -- instead of showing stale text. Skip describe.jj so an in-progress
      -- commit message isn't discarded.
      local fname = vim.uri_to_fname(result)
      if not fname:match('/describe%.jj$') then
        pcall(vim.cmd, 'silent! edit!')
      end
    end)
  end)
end

return M
