if vim.g.loaded_badjuju then
  return
end
vim.g.loaded_badjuju = 1

require('badjuju.commands').register_all()
