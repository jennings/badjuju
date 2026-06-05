local badjuju = require('badjuju')

-- Stub vim.lsp.util.show_document and capture invocations of vim.cmd
-- (specifically 'checktime'). Replace badjuju.ensure_client with a fake
-- client whose :request fires the callback synchronously with a chosen
-- `result`. M.execute's body runs under vim.schedule, so tests wait for
-- the schedule to drain via vim.wait.

local function run_execute(opts)
  local show_doc_calls = {}
  local cmd_calls = {}
  local after_calls = {}
  local fake_uri = opts.result_uri

  local original_show = vim.lsp.util.show_document
  local original_cmd = vim.cmd
  local original_ensure = badjuju.ensure_client
  local original_uri_to_fname = vim.uri_to_fname

  vim.lsp.util.show_document = function(loc, encoding, o)
    table.insert(show_doc_calls, { loc = loc, encoding = encoding, opts = o })
    return true
  end
  -- vim.cmd can be either called as a function (vim.cmd('foo')) or indexed
  -- (vim.cmd.split()). M.execute uses the function form for 'split',
  -- 'vsplit', 'edit ...', and pcall(vim.cmd, 'checktime'). Stub it as a
  -- callable that records the invocation; preserve indexed access to the
  -- original for anything else under test.
  vim.cmd = setmetatable({}, {
    __call = function(_, arg)
      table.insert(cmd_calls, arg)
    end,
    __index = function(_, k)
      return original_cmd[k]
    end,
  })

  badjuju.ensure_client = function()
    return {
      offset_encoding = 'utf-16',
      request = function(_, method, params, cb)
        cb(nil, fake_uri)
      end,
    }
  end

  badjuju.execute(opts.command or 'badjuju.squash.commit', opts.args or {}, {
    split = opts.split,
    after = function(uri)
      table.insert(after_calls, uri)
    end,
  })

  -- Drain vim.schedule.
  vim.wait(200, function()
    return #after_calls > 0 or #show_doc_calls > 0 or #cmd_calls > 0
  end, 10)

  -- Restore.
  vim.lsp.util.show_document = original_show
  vim.cmd = original_cmd
  badjuju.ensure_client = original_ensure
  vim.uri_to_fname = original_uri_to_fname

  return {
    show_doc = show_doc_calls,
    cmd = cmd_calls,
    after = after_calls,
  }
end

local counter = 0

--- Set up a fresh buffer named at `path` and return the URI Neovim actually
--- stores for it. Neovim resolves symlinks on `nvim_buf_set_name` (so /var
--- becomes /private/var on macOS), and M.execute compares against the
--- resolved buffer name — so callers must use the returned URI as the
--- "current buffer URI" rather than re-deriving it from the input path.
local function set_current_buffer_to_path(path)
  counter = counter + 1
  vim.fn.mkdir(vim.fn.fnamemodify(path, ':h'), 'p')
  vim.cmd.enew()
  vim.api.nvim_buf_set_name(0, path)
  return vim.uri_from_fname(vim.api.nvim_buf_get_name(0))
end

local function checktime_was_called(calls)
  for _, c in ipairs(calls.cmd) do
    if type(c) == 'string' and c == 'checktime' then
      return true
    end
  end
  return false
end

describe('badjuju.execute refocus short-circuit', function()
  it('skips show_document but calls checktime when result URI matches current buffer', function()
    -- Same-URI response: the server returned the URI of the buffer the user
    -- is already in. show_document would re-fire BufReadPost and collapse
    -- folds, so it must be skipped. But checktime MUST still fire so the
    -- buffer reloads when the server actually rewrote the file (which is
    -- the case for almost every mutating command — bookmark, refresh,
    -- abandon, undo, push, fetch, edit, rebase, new, next, prev, unsquash).
    -- Regression test for #72: pressing bc/bm/bd or R in the status buffer
    -- did nothing because this branch returned without calling checktime.
    local tmp = vim.fn.tempname() .. '-refocus' .. counter
    local status_uri = set_current_buffer_to_path(tmp .. '/.jj/badjuju/status.jujutsu')

    local calls = run_execute({ result_uri = status_uri })

    assert.are.equal(0, #calls.show_doc,
      'show_document should NOT be called for refocus-of-current-buffer')
    assert.is_true(checktime_was_called(calls),
      'vim.cmd("checktime") MUST be called so a server-rewritten status buffer reloads')
    assert.are.equal(1, #calls.after, 'after callback should fire exactly once')
    assert.are.equal(status_uri, calls.after[1])
  end)

  it('does NOT call checktime when result URI is the describe buffer', function()
    -- describe.jujutsu is the one buffer where checktime is unsafe: the user
    -- may be editing an unsaved commit message, and reloading would discard
    -- their draft. The exclusion matches the same-buffer-different-uri branch
    -- below.
    local tmp = vim.fn.tempname() .. '-refocus' .. counter
    local describe_uri = set_current_buffer_to_path(tmp .. '/.jj/badjuju/describe.jujutsu')

    local calls = run_execute({ result_uri = describe_uri })

    assert.is_false(checktime_was_called(calls),
      'checktime must NOT fire when the result URI is describe.jujutsu')
    assert.are.equal(0, #calls.show_doc)
  end)

  it('does NOT short-circuit when split is requested', function()
    local tmp = vim.fn.tempname() .. '-refocus' .. counter
    local status_uri = set_current_buffer_to_path(tmp .. '/.jj/badjuju/status.jujutsu')

    local calls = run_execute({
      result_uri = status_uri,
      split = 'h',
    })

    -- A split was requested, so we should still open the result URI.
    assert.are.equal(1, #calls.show_doc,
      'show_document should fire when split is requested even for same URI')
  end)

  it('opens normally when result URI differs from current buffer', function()
    local tmp = vim.fn.tempname() .. '-refocus' .. counter
    set_current_buffer_to_path(tmp .. '/.jj/badjuju/status.jujutsu')

    -- Pretend the server returned a squash buffer URI (different from current).
    -- Note: vim.uri_from_fname doesn't resolve symlinks, so this URI does not
    -- need to be post-processed — we never compare it to a buffer name.
    local squash_uri = vim.uri_from_fname(tmp .. '/.jj/badjuju/squash/aa-bb.jujutsu')

    local calls = run_execute({ result_uri = squash_uri })

    assert.are.equal(1, #calls.show_doc,
      'show_document should fire for a different result URI')
    assert.are.equal(squash_uri, calls.show_doc[1].loc.uri)
    -- after also fires.
    assert.are.equal(1, #calls.after)
    assert.are.equal(squash_uri, calls.after[1])
  end)
end)

-- Regression test for #72. Every state-changing badjuju command invoked from
-- status.jujutsu returns the status URI as its result. The client used to
-- short-circuit on a same-URI match and skip checktime, which left the
-- status buffer stale on disk reloads. This spec enumerates every such
-- command and asserts that M.execute calls checktime for each. Adding a new
-- mutating command that returns the status URI should require no new test
-- here — but if M.execute's same-URI branch ever regresses, every entry
-- here fails at once, making the broken contract impossible to miss.
describe('badjuju.execute refresh-after-mutation (#72)', function()
  local STATE_CHANGING_COMMANDS = {
    'badjuju.bookmark',
    'badjuju.refresh',
    'badjuju.abandon',
    'badjuju.undo',
    'badjuju.push',
    'badjuju.fetch',
    'badjuju.edit',
    'badjuju.rebase',
    'badjuju.new',
    'badjuju.next',
    'badjuju.prev',
    'badjuju.unsquash',
  }

  for _, command in ipairs(STATE_CHANGING_COMMANDS) do
    it(command .. ' triggers checktime when run from status.jujutsu', function()
      local tmp = vim.fn.tempname() .. '-mutate' .. counter
      local status_uri = set_current_buffer_to_path(tmp .. '/.jj/badjuju/status.jujutsu')

      local calls = run_execute({
        command = command,
        result_uri = status_uri,
      })

      assert.is_true(checktime_was_called(calls),
        command .. ': checktime must fire so the status buffer reloads the new content')
    end)
  end
end)
