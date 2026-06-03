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

describe('badjuju.execute refocus short-circuit', function()
  it('skips show_document and checktime when result URI matches current buffer', function()
    local tmp = vim.fn.tempname() .. '-refocus' .. counter
    local status_uri = set_current_buffer_to_path(tmp .. '/.jj/badjuju/status.jujutsu')

    local calls = run_execute({ result_uri = status_uri })

    assert.are.equal(0, #calls.show_doc,
      'show_document should NOT be called for refocus-of-current-buffer')
    -- Walk cmd_calls and verify no 'checktime' string was passed.
    for _, c in ipairs(calls.cmd) do
      assert.is_falsy(
        type(c) == 'string' and c == 'checktime',
        'vim.cmd should NOT have been called with checktime'
      )
    end
    -- after callback still fires with the result URI.
    assert.are.equal(1, #calls.after, 'after callback should fire exactly once')
    assert.are.equal(status_uri, calls.after[1])
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
