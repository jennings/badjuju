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

-- Graph node bytes that introduce a commit header line in jj log output.
-- Mirrors the JS COMMIT_HEADER_RE class in clients/vscode/src/extension.ts.
-- Does NOT include `~` (the elided-continuation marker).
local COMMIT_HEADER_GRAPH_CHARS = {
  '@',
  '%*', -- literal '*' escaped for Lua patterns
  '\xE2\x97\x8B', -- ○
  '\xE2\x97\x8F', -- ●
  '\xE2\x97\x86', -- ◆
}

--- Match a commit-header line (graph char + spaces + change_id). Returns the
--- change_id, or nil if `line` is not a commit header.
local function match_commit_header(line)
  if not line then return nil end
  for _, ch in ipairs(COMMIT_HEADER_GRAPH_CHARS) do
    local change_id = line:match('^' .. ch .. '%s+(%l+)')
    if change_id then
      return change_id
    end
  end
  return nil
end

--- Return the revision that owns the line at `cursor_line` (0-indexed) in a
--- status.jujutsu buffer. Mirrors findRevisionForLine in extension.ts.
---
--- - A STATUS-section file line (`M file`, `A file`, etc.) belongs to the
---   working copy → `@`.
--- - Otherwise walk up from `cursor_line` (inclusive) until we hit a commit
---   header line; return that commit's change_id.
--- - Hitting the `STATUS:` section header without finding a commit means we
---   were in the STATUS file list with no commit context → working copy.
---@param lines string[]
---@param cursor_line integer  0-indexed
---@return string
function M.find_revision_for_line(lines, cursor_line)
  local current = lines[cursor_line + 1] or ''
  if current:match(STATUS_FILE_PATTERN) then
    return '@'
  end
  for i = cursor_line, 0, -1 do
    local text = lines[i + 1] or ''
    local change_id = match_commit_header(text)
    if change_id then
      return change_id
    end
    if text:sub(1, 7) == 'STATUS:' then
      return '@'
    end
  end
  return '@'
end

--- Return the change_id of the commit at or above the cursor in a log.jujutsu
--- buffer. Mirrors findLogRevision in extension.ts.
---
--- Walks up from `cursor_line` (inclusive) looking for a commit header line.
--- Returns nil if none is found (e.g. cursor in the REVSET header section).
---@param lines string[]
---@param cursor_line integer  0-indexed
---@return string?
function M.find_log_revision(lines, cursor_line)
  for i = cursor_line, 0, -1 do
    local text = lines[i + 1] or ''
    local change_id = match_commit_header(text)
    if change_id then
      return change_id
    end
  end
  return nil
end

--- Parse a `status.jujutsu` line and return the file path it refers to, or nil.
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
