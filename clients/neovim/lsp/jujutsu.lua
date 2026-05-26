local ok, badjuju = pcall(require, 'badjuju')
local config = (ok and badjuju.config) or {}

local init_options = {
  keymapProfile = (config.keymap_profile and config.keymap_profile ~= '') and config.keymap_profile or 'magit',
  -- Signal that this client supports workspace/textDocumentContent so the server
  -- returns virtual badjuju-diff: URIs instead of writing files to disk.
  virtualDiffs = true,
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
