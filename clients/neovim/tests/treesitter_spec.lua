-- Verifies queries/jujutsu/highlights.scm by building the local grammar
-- with the tree-sitter CLI, loading it via vim.treesitter.language.add, and
-- asserting that every expected capture name lands on the expected text.
--
-- On machines without the tree-sitter CLI (or when the build itself fails)
-- the tests pend rather than fail, matching the same skip-on-missing-tooling
-- behavior as tree-sitter-jujutsu/test.do.

local plugin_root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h')

local function build_parser()
  if vim.fn.executable('tree-sitter') == 0 then
    return nil, 'tree-sitter CLI not installed'
  end
  local grammar_dir = plugin_root .. '/tree-sitter-jujutsu'
  if vim.fn.isdirectory(grammar_dir) == 0 then
    return nil, 'grammar directory missing at ' .. grammar_dir
  end
  local out_so = vim.fn.tempname() .. '.so'
  local result = vim.fn.system({ 'tree-sitter', 'build', '-o', out_so, grammar_dir })
  if vim.v.shell_error ~= 0 then
    return nil, 'tree-sitter build failed: ' .. result
  end
  return out_so
end

local parser_so, skip_reason = build_parser()
if parser_so then
  local ok, err = pcall(vim.treesitter.language.add, 'jujutsu', { path = parser_so })
  if not ok then
    parser_so = nil
    skip_reason = 'vim.treesitter.language.add failed: ' .. tostring(err)
  end
end

local fixture_lines = {
  'JJ: this is a comment',
  'STATUS: workspace summary',
  '@  qpvuntsm 1234abcd master',
  '│  (empty) (no description set)',
  '◆  abcdefgh 5678abcd',
  'M src/main.rs',
  'A new.txt',
  '○  zzzzwxyz 0edcba9876543210 [feature]',
  '',
}
local fixture = table.concat(fixture_lines, '\n')

local function collect_captures()
  local query = vim.treesitter.query.get('jujutsu', 'highlights')
  assert.is_not_nil(query, 'highlights query not found on runtimepath')
  local parser = vim.treesitter.get_string_parser(fixture, 'jujutsu')
  local root = parser:parse()[1]:root()
  local results = {}
  for id, node in query:iter_captures(root, fixture, 0, -1) do
    local sr, sc, er, ec = node:range()
    table.insert(results, {
      name = query.captures[id],
      sr = sr,
      sc = sc,
      er = er,
      ec = ec,
      text = vim.treesitter.get_node_text(node, fixture),
    })
  end
  return results
end

local function find_capture(captures, name, row, text)
  for _, c in ipairs(captures) do
    if c.name == name and c.sr == row and (text == nil or c.text == text) then
      return c
    end
  end
  return nil
end

local function assert_capture(captures, name, row, text)
  local c = find_capture(captures, name, row, text)
  assert.is_not_nil(
    c,
    string.format('expected @%s at row %d covering %q', name, row, text or '(any)')
  )
end

local function pend_or(it_name, body)
  it(it_name, function()
    if not parser_so then
      pending('parser not built: ' .. tostring(skip_reason))
      return
    end
    body()
  end)
end

describe('jujutsu highlights query', function()
  pend_or('jj_comment maps to @comment', function()
    local caps = collect_captures()
    assert_capture(caps, 'comment', 0)
  end)

  pend_or('section_header header maps to @keyword, trailing to @string', function()
    local caps = collect_captures()
    assert_capture(caps, 'keyword', 1, 'STATUS:')
    assert_capture(caps, 'string', 1, ' workspace summary')
  end)

  pend_or('file_status maps to @type', function()
    local caps = collect_captures()
    assert_capture(caps, 'type', 5, 'M ')
    assert_capture(caps, 'type', 6, 'A ')
  end)

  pend_or('empty_marker maps to @comment.note', function()
    local caps = collect_captures()
    assert_capture(caps, 'comment.note', 3, '(empty)')
    assert_capture(caps, 'comment.note', 3, '(no description set)')
  end)

  pend_or('bookmark maps to @tag', function()
    local caps = collect_captures()
    assert_capture(caps, 'tag', 7, '[feature]')
  end)

  pend_or('commit_id and change_id both map to @number', function()
    local caps = collect_captures()
    assert_capture(caps, 'number', 2, 'qpvuntsm')
    assert_capture(caps, 'number', 2, '1234abcd')
    assert_capture(caps, 'number', 4, 'abcdefgh')
    assert_capture(caps, 'number', 4, '5678abcd')
    assert_capture(caps, 'number', 7, 'zzzzwxyz')
    assert_capture(caps, 'number', 7, '0edcba9876543210')
  end)

  pend_or('graph_char maps to @punctuation.special', function()
    local caps = collect_captures()
    assert_capture(caps, 'punctuation.special', 2, '@')
    assert_capture(caps, 'punctuation.special', 3, '│')
    assert_capture(caps, 'punctuation.special', 4, '◆')
    assert_capture(caps, 'punctuation.special', 7, '○')
  end)
end)
