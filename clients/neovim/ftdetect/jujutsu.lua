-- Register the jujutsu filetype for `*.jujutsu` files. Using vim.filetype.add
-- (instead of the old `autocmd ... set filetype=...` form) ensures our
-- detection takes priority over any conflicting built-in mapping in
-- runtime/lua/vim/filetype.lua.
vim.filetype.add({
  extension = {
    jujutsu = 'jujutsu',
  },
})
