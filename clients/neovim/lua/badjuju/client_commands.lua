-- Client-side handlers for the prompt-requiring code actions the server
-- emits (`badjuju.client.rebasePrompt`, `badjuju.client.bookmarkPrompt`).
-- Server-side code actions can't know a rebase destination or bookmark name,
-- so the action ships a pre-resolved revision and the client prompts for the
-- missing piece before forwarding to the real server command.
--
-- Routing: vim.lsp.commands[name] is consulted by Neovim's LSP code-action
-- pipeline before falling back to workspace/executeCommand. Registering here
-- intercepts the call client-side. See `:help lsp.commands`.

local M = {}

local function execute(command, arguments)
  require('badjuju').execute(command, arguments)
end

--- Handle the `badjuju.client.rebasePrompt` code-action command. The first
--- argument is the source revision pre-resolved by the server.
local function prompt_rebase(command, _ctx)
  local revision = (command.arguments or {})[1] or '@'
  vim.ui.input({ prompt = 'Rebase to: ' }, function(dest)
    if not dest or dest == '' then return end
    execute('badjuju.rebase', { revision, dest })
  end)
end

--- Handle the `badjuju.client.bookmarkPrompt` code-action command. Prompts
--- for sub_action and name; the revision argument is consumed only by
--- create/move sub-actions.
local function prompt_bookmark(command, _ctx)
  local revision = (command.arguments or {})[1] or '@'
  local ACTIONS = { 'create', 'move', 'delete', 'track', 'forget' }
  vim.ui.select(ACTIONS, { prompt = 'jj bookmark: ' }, function(sub_action)
    if not sub_action then return end
    local prompt = sub_action == 'track'
      and 'Bookmark (e.g. main@origin): '
      or 'Bookmark name: '
    vim.ui.input({ prompt = prompt }, function(bname)
      if not bname or bname == '' then return end
      local needs_rev = sub_action == 'create' or sub_action == 'move'
      local rev = needs_rev and revision or ''
      execute('badjuju.bookmark', { sub_action, bname, rev })
    end)
  end)
end

--- Install the client-side handlers in the global vim.lsp.commands table.
--- Idempotent: overwrites any prior registration with the same name.
function M.register()
  vim.lsp.commands['badjuju.client.rebasePrompt'] = prompt_rebase
  vim.lsp.commands['badjuju.client.bookmarkPrompt'] = prompt_bookmark
end

return M
