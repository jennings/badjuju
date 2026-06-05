local M = {}

local DIFF_SCHEME = 'badjuju-diff'
local FILE_SCHEME = 'badjuju-file'
local FILELOG_SCHEME = 'badjuju-filelog'

--- Populate a buffer with content fetched via workspace/textDocumentContent.
--- The handler is scheme-agnostic — it forwards whatever URI it receives.
--- The caller is responsible for setting the appropriate filetype.
---@param bufnr integer
---@param uri string  full badjuju-diff:// or badjuju-file:// URI
---@param filetype string?  filetype to set; nil to infer from URI path
local function populate_virtual_buf(bufnr, uri, filetype)
  local client = M.get_client()
  if not client then return end
  local result = client.request_sync('workspace/textDocumentContent', { uri = uri }, 5000, bufnr)
  if not result or result.err or not result.result then return end
  local text = (result.result.text or '')
  local lines = vim.split(text, '\n', { plain = true })
  vim.bo[bufnr].modifiable = true
  vim.bo[bufnr].readonly = false
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
  vim.bo[bufnr].buftype = 'nofile'
  vim.bo[bufnr].bufhidden = 'wipe'
  vim.bo[bufnr].modifiable = false
  vim.bo[bufnr].readonly = true
  if filetype then
    vim.bo[bufnr].filetype = filetype
  else
    -- Infer from the repo-relative path encoded in the badjuju-file:// URI
    -- (everything after /commit/<id>/) so editors highlight .rs as Rust, etc.
    local path = uri:match('^badjuju%-file:/+commit/[^/]+/(.+)$')
    if path then
      local ft = vim.filetype.match({ filename = path, buf = bufnr })
      if ft then vim.bo[bufnr].filetype = ft end
    end
  end
  vim.b[bufnr].badjuju_diff_uri = uri
end

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

  -- BufReadCmd polyfill for virtual badjuju-* URIs. Fires whenever Neovim
  -- tries to read a buffer whose name matches one of the badjuju virtual
  -- schemes. Fetches content from the server via workspace/textDocumentContent.
  vim.api.nvim_create_autocmd('BufReadCmd', {
    pattern = DIFF_SCHEME .. '://*',
    callback = function(args)
      populate_virtual_buf(args.buf, args.file, 'jujutsu')
    end,
  })
  vim.api.nvim_create_autocmd('BufReadCmd', {
    pattern = FILE_SCHEME .. '://*',
    callback = function(args)
      -- nil filetype → infer from the URI's encoded path so .rs maps to rust, etc.
      populate_virtual_buf(args.buf, args.file, nil)
    end,
  })
  vim.api.nvim_create_autocmd('BufReadCmd', {
    pattern = FILELOG_SCHEME .. '://*',
    callback = function(args)
      populate_virtual_buf(args.buf, args.file, 'jujutsu')
    end,
  })

  -- Global handler for workspace/textDocumentContent/refresh (server → client).
  -- The server sends this after each mutation for every open change-mode diff.
  -- file-blob buffers are never refreshed — commit-id pins them — so the
  -- refresh handler only re-fetches with the diff filetype.
  vim.lsp.handlers['workspace/textDocumentContent/refresh'] = function(_, params)
    if type(params) ~= 'table' or type(params.uri) ~= 'string' then return end
    local uri = params.uri
    for _, bufnr in ipairs(vim.api.nvim_list_bufs()) do
      if vim.api.nvim_buf_is_loaded(bufnr)
          and vim.api.nvim_buf_get_name(bufnr) == uri then
        populate_virtual_buf(bufnr, uri, 'jujutsu')
      end
    end
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
---@param opts { after?: fun(result_uri: string), split?: 'h'|'v', on_error?: fun(err: table) }?
---   after: optional callback invoked on the main loop after the result file
---     has been opened and reloaded. Receives the server-returned URI string.
---   split: open the result file in a new horizontal ('h') or vertical ('v')
---     split window instead of replacing the current window's buffer.
---   on_error: optional callback receiving the raw jsonrpc error table (has
---     `.message` and optionally `.data`). When provided, suppresses the
---     default vim.notify error display.
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
      if opts and opts.on_error then
        vim.schedule(function() opts.on_error(err) end)
      else
        vim.notify('badjuju: ' .. (err.message or vim.inspect(err)), vim.log.levels.ERROR)
      end
      return
    end
    if type(result) ~= 'string' or result == '' then
      return
    end
    vim.schedule(function()
      -- Refocus-only responses: commands like badjuju.squash.commit (source
      -- selection) and badjuju.squash.cancel return the cursor's URI to let
      -- the client refocus its existing buffer. When that URI matches the
      -- buffer the user is already in and no split was requested, opening it
      -- via show_document + checktime is at best a no-op — and at worst
      -- triggers a BufReadPost that re-runs ftplugin/jujutsu.lua, which
      -- closes every user-opened fold. Skip the open path; still fire `after`.
      local opens_window = opts and (opts.split == 'h' or opts.split == 'v')
      local current_uri = vim.uri_from_fname(vim.api.nvim_buf_get_name(0))
      if not opens_window and result == current_uri then
        if opts and opts.after then
          opts.after(result)
        end
        return
      end
      if opts and opts.split == 'h' then
        vim.cmd('split')
      elseif opts and opts.split == 'v' then
        vim.cmd('vsplit')
      end
      -- Virtual diff / filelog URIs: open as a nofile buffer populated via BufReadCmd.
      if
        result:sub(1, #DIFF_SCHEME + 3) == DIFF_SCHEME .. '://'
        or result:sub(1, #FILELOG_SCHEME + 3) == FILELOG_SCHEME .. '://'
      then
        vim.cmd('edit ' .. vim.fn.fnameescape(result))
        if opts and opts.after then
          opts.after(result)
        end
        return
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
        pcall(vim.cmd, 'checktime')
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
