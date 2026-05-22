local ok, badjuju = pcall(require, 'badjuju')
local config = (ok and badjuju.config) or {}

local init_options = {
  keymapProfile = 'magit',
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
