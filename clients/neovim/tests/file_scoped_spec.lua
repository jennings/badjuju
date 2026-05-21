local parse = require('badjuju.parse')

-- Sample status.jujutsu rendered by the server. Includes a STATUS file list
-- (working copy) and a STACK section with commit headers and stat lines.
local SAMPLE = {
  'STATUS:',
  '',
  'Working copy changes:',
  'M src/main.rs',
  'A src/new.rs',
  '',
  'STACK: ancestors(reachable(@, mutable()), 2)',
  '',
  '@  kpkzwvqm 909679d0 stephen@example.com',
  '│  (empty) (no description set)',
  '│  src/main.rs | 3 +++',
  '○  xorwskru 66bfbfdf stephen@example.com',
  '│  feat: a change',
  '│  src/feature.rs | 5 ++---',
  '◆  spxlzwpr 18d66a82 stephen@example.com main',
  '│  fix: an immutable change',
  '~',
}

-- 0-indexed line lookup helpers tied to SAMPLE for clarity.
local function line_of(text)
  for i, l in ipairs(SAMPLE) do
    if l == text then return i - 1 end
  end
  error('not found in SAMPLE: ' .. tostring(text))
end

describe('parse.find_revision_for_line', function()
  it('returns @ on a STATUS-section M line', function()
    assert.are.equal('@', parse.find_revision_for_line(SAMPLE, line_of('M src/main.rs')))
  end)

  it('returns @ on a STATUS-section A line', function()
    assert.are.equal('@', parse.find_revision_for_line(SAMPLE, line_of('A src/new.rs')))
  end)

  it('returns the working-copy change_id on its own header line', function()
    -- Cursor on the @ commit header — walks up and matches itself.
    assert.are.equal('kpkzwvqm', parse.find_revision_for_line(SAMPLE, line_of('@  kpkzwvqm 909679d0 stephen@example.com')))
  end)

  it('returns the parent change_id on a stat line beneath ○', function()
    assert.are.equal('xorwskru', parse.find_revision_for_line(SAMPLE, line_of('│  src/feature.rs | 5 ++---')))
  end)

  it('returns the immutable commit change_id on its own header line', function()
    assert.are.equal('spxlzwpr', parse.find_revision_for_line(SAMPLE, line_of('◆  spxlzwpr 18d66a82 stephen@example.com main')))
  end)

  it('returns @ for a STACK header above the first commit (no commit found, hits STATUS:)', function()
    -- "STACK: ..." line itself: walks up past blank line and STATUS:, no commit, fall back to @.
    assert.are.equal('@', parse.find_revision_for_line(SAMPLE, line_of('STACK: ancestors(reachable(@, mutable()), 2)')))
  end)

  it('returns @ for a stat line on the working-copy commit', function()
    -- Cursor on the stat line under @. Should resolve up to @ commit header
    -- which has change_id `kpkzwvqm`.
    assert.are.equal('kpkzwvqm', parse.find_revision_for_line(SAMPLE, line_of('│  src/main.rs | 3 +++')))
  end)
end)

describe('status.jujutsu s/U buffer maps', function()
  local keymap = require('badjuju.keymap')

  local counter = 0
  local function unique(rel)
    counter = counter + 1
    return vim.fn.tempname() .. '-fs' .. counter .. '/' .. rel
  end

  local function has_buffer_map(bufnr, key)
    for _, m in ipairs(vim.api.nvim_buf_get_keymap(bufnr, 'n')) do
      if m.lhs == key then return true end
    end
    return false
  end

  it('binds s and U on status.jujutsu', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique('.jj/badjuju/status.jujutsu'))
    keymap.setup_for_buffer(0)
    local buf = vim.api.nvim_get_current_buf()
    assert.is_true(has_buffer_map(buf, 's'), 'expected s map on status.jujutsu')
    assert.is_true(has_buffer_map(buf, 'U'), 'expected U map on status.jujutsu')
  end)

  it('does NOT bind s or U on log.jujutsu', function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_name(0, unique('.jj/badjuju/log.jujutsu'))
    keymap.setup_for_buffer(0)
    local buf = vim.api.nvim_get_current_buf()
    assert.is_false(has_buffer_map(buf, 's'), 'log.jujutsu should not bind s')
    assert.is_false(has_buffer_map(buf, 'U'), 'log.jujutsu should not bind U')
  end)
end)
