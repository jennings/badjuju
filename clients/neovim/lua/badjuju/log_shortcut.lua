local M = {}

-- Port of the VS Code LOG_SHORTCUT_LINE_RE: /^JJ:\s+([A-Za-z][\w ]*?):\s+(.+)$/.
-- Matches the shortcut lines rendered by server/src/commands.rs::render_log_shortcuts,
-- e.g. "JJ: Mutable:  ancestors(reachable(@, mutable()))".
local PATTERN = '^JJ:%s+(%a[%w _]-):%s+(.+)$'

--- Parse a `log.jujutsu` shortcut line. Returns the (label, revset) pair, or
--- (nil, nil) when the line is not a shortcut.
---@param line string?
---@return string?, string?
function M.parse(line)
  if not line or line == '' then
    return nil, nil
  end
  local label, revset = line:match(PATTERN)
  if not label or not revset then
    return nil, nil
  end
  return label, (revset:gsub('%s+$', ''))
end

return M
