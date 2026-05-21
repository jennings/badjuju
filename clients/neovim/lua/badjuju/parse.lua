local M = {}

-- Matches "M src/main.rs", "A new.txt", etc.
local STATUS_FILE_PATTERN = '^([MADCR])%s+(.+)$'

-- Matches "5 files changed, 30 insertions(+), 2 deletions(-)" — used to reject
-- the stat summary line that would otherwise look path-like.
local STAT_SUMMARY_PATTERN = '^%s*%d+%s+files?%s+changed'

local function strip_stat_suffix(line)
  -- A jj `--stat` per-file line ends with " | <num> <+/->" (`.../foo.rs | 3 +++`).
  -- This returns the portion before that suffix, or nil if the line lacks it.
  return line:match('^(.+)%s|%s+%d+%s+[+-]+%s*$')
end

-- Each entry is the full UTF-8 byte sequence of one graph character that can
-- appear in the stat-line prefix. Mirrors the JS character class in
-- clients/vscode/src/extension.ts (STAT_LINE_RE).
local STAT_PREFIX_GRAPH_CHARS = {
  '\xE2\x94\x82', -- │
  '\xE2\x97\x8B', -- ○
  '\xE2\x97\x8F', -- ●
  '\xE2\x97\x86', -- ◆
  '\xE2\x95\xAD', -- ╭
  '\xE2\x95\xAE', -- ╮
  '\xE2\x95\xAF', -- ╯
  '\xE2\x95\xB0', -- ╰
  '\xE2\x94\x80', -- ─
  '\xE2\x94\x9C', -- ├
  '\xE2\x94\xA4', -- ┤
  '\xE2\x94\xAC', -- ┬
  '\xE2\x94\xB4', -- ┴
  '\xE2\x94\xBC', -- ┼
}

local function strip_stat_prefix(s)
  -- Repeatedly strip whitespace and known graph characters from the left.
  while true do
    local before = s
    s = s:gsub('^%s+', '')
    s = s:gsub('^[~%*]', '', 1)
    for _, ch in ipairs(STAT_PREFIX_GRAPH_CHARS) do
      s = s:gsub('^' .. ch, '', 1)
    end
    if s == before then
      return s
    end
  end
end

local function trim(s)
  return (s:gsub('^%s+', ''):gsub('%s+$', ''))
end

local function strip_rename_arrow(path)
  -- jj renders renames/copies as "old => new" — squash needs the destination path.
  local last = nil
  local pos = 1
  while true do
    local s, e = path:find(' => ', pos, true)
    if not s then break end
    last = s
    pos = e + 1
  end
  if last then
    return trim(path:sub(last + 4))
  end
  return path
end

--- Parse a `status.jj` line and return the file path it refers to, or nil.
---
--- Handles two line shapes:
---   1. Status header lines like `M src/main.rs` (one of M/A/D/C/R + path).
---   2. `jj log --stat` lines like `│  src/main.rs | 3 +++` (returns the path).
---
--- Returns nil for blank lines, summary lines, headers, and anything else.
---@param line string
---@return string?
function M.parse_status_file(line)
  if not line or line == '' then
    return nil
  end

  -- Try the simple status header form first.
  local _flag, rest = line:match(STATUS_FILE_PATTERN)
  if rest then
    return strip_rename_arrow(trim(rest))
  end

  -- Otherwise try the stat-line form. The "N files changed, ..." summary
  -- line looks superficially similar but lacks the per-file " | N +/-" tail,
  -- so it won't match strip_stat_suffix; we still guard against pathological
  -- summary lines just in case.
  if line:match(STAT_SUMMARY_PATTERN) then
    return nil
  end
  local path_with_prefix = strip_stat_suffix(line)
  if not path_with_prefix then
    return nil
  end
  local path = strip_stat_prefix(path_with_prefix)
  path = trim(path)
  if path == '' then
    return nil
  end
  return strip_rename_arrow(path)
end

return M
