local parse = require('badjuju.parse')

describe('find_log_revision', function()
  it('returns the change_id on a commit header line', function()
    local lines = { '@  qpvuntsm 1234abcd', '   description here' }
    assert.are.equal('qpvuntsm', parse.find_log_revision(lines, 0))
  end)

  it('walks up to the nearest commit header', function()
    local lines = {
      '@  qpvuntsm 1234abcd',
      '   first line',
      '   second line',
    }
    assert.are.equal('qpvuntsm', parse.find_log_revision(lines, 2))
  end)

  it('returns nil when no commit header is above the cursor', function()
    local lines = { 'REVSET: @', 'JJ: comment', '' }
    assert.is_nil(parse.find_log_revision(lines, 2))
  end)

  it('handles non-current commit symbols (○/●/◆)', function()
    local lines = { '○  abcdwxyz e6f7' }
    assert.are.equal('abcdwxyz', parse.find_log_revision(lines, 0))
  end)
end)

describe('parse_status_file', function()
  it('returns nil for blank lines', function()
    assert.is_nil(parse.parse_status_file(''))
  end)

  it('returns nil for nil input', function()
    assert.is_nil(parse.parse_status_file(nil))
  end)

  it('parses a plain status line (M)', function()
    assert.are.equal('src/main.rs', parse.parse_status_file('M src/main.rs'))
  end)

  it('parses A/D/C/R status flags', function()
    assert.are.equal('new.txt', parse.parse_status_file('A new.txt'))
    assert.are.equal('gone.txt', parse.parse_status_file('D gone.txt'))
    assert.are.equal('copied.txt', parse.parse_status_file('C copied.txt'))
    assert.are.equal('renamed.txt', parse.parse_status_file('R renamed.txt'))
  end)

  it('handles paths with spaces', function()
    assert.are.equal('a b/c d.txt', parse.parse_status_file('M a b/c d.txt'))
  end)

  it('parses a stat-graph line', function()
    assert.are.equal('src/main.rs', parse.parse_status_file('│  src/main.rs | 3 +++'))
  end)

  it('parses a stat-graph line with deep graph chars', function()
    assert.are.equal('foo.rs', parse.parse_status_file('├─╮  foo.rs | 1 +'))
  end)

  it('returns the destination of a rename', function()
    assert.are.equal('new/path.rs', parse.parse_status_file('R old/path.rs => new/path.rs'))
  end)

  it('returns nil for the stat summary line', function()
    assert.is_nil(parse.parse_status_file('5 files changed, 30 insertions(+), 2 deletions(-)'))
  end)

  it('returns nil for a single-file stat summary', function()
    assert.is_nil(parse.parse_status_file('1 file changed, 3 insertions(+)'))
  end)

  it('returns nil for an unrelated header line', function()
    assert.is_nil(parse.parse_status_file('Working copy changes:'))
  end)

  it('returns nil for plain commit-id text', function()
    assert.is_nil(parse.parse_status_file('@  qpvuntsm 1234abcd'))
  end)
end)
