local ok, badjuju = pcall(require, 'badjuju')
local config = (ok and badjuju.config) or {}

-- Neovim-flavored COMMAND REFERENCE blocks rendered at the bottom of each
-- generated buffer. The server's built-in defaults assume VS Code-style
-- hotkeys (Ctrl+n / Ctrl+Shift+n, etc.) which don't exist in this plugin's
-- keymap.lua; sending these overrides keeps the in-buffer cheatsheet honest.
local STATUS_REFERENCE = table.concat({
  'COMMAND REFERENCE:',
  'n   new change',
  'l   open log',
  'd   describe commit at cursor (opens in a split)',
  'D   diff commit at cursor (opens in a split)',
  's   squash file at cursor into parent',
  'U   unsquash file at cursor from parent into child',
  'a   abandon commit at cursor (or working copy)',
  'u   jj undo (revert last operation)',
  '=   toggle --stat on the stack log',
  'g   refresh   (or r)',
  'q   close',
}, '\n')

local LOG_REFERENCE = table.concat({
  'COMMAND REFERENCE:',
  'Edit REVSET above and save to re-run the query.',
  'Place the cursor on a shortcut line and press <CR> to apply it.',
  'd   describe commit at cursor (opens in a split)',
  'D   diff commit at cursor (opens in a split)',
  'a   abandon commit at cursor',
  'g   refresh   (or r)',
}, '\n')

local init_options = {
  commandReference = {
    status = STATUS_REFERENCE,
    log = LOG_REFERENCE,
    -- diff: omit so the server falls back to its built-in (which matches
    -- this plugin's diff keymap: g/r refresh, q close).
  },
}
if config.binary_path and config.binary_path ~= '' then
  init_options.binaryPath = config.binary_path
end

return {
  cmd = { 'badjuju', 'lsp' },
  filetypes = { 'jujutsu' },
  root_markers = { '.jj' },
  init_options = init_options,
}
