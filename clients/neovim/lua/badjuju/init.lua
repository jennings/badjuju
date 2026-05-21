local M = {}

M.config = {
  default_log_revset = nil,
  binary_path = nil,
}

local CLIENT_NAME = 'jujutsu'

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
    end)
  end)
end

return M
