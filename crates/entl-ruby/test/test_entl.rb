# Smoke test for the Ruby (Magnus) binding. Build first:
#   cd crates/entl-ruby && ruby extconf.rb && make
# Run:
#   ruby -Icrates/entl-ruby -Icrates/entl-ruby/test crates/entl-ruby/test/test_entl.rb
require "minitest/autorun"
require "json"
require "tmpdir"
require "fileutils"
require "entl"

class TestEntl < Minitest::Test
  def fixture_repo(dir)
    repo = File.join(dir, "repo")
    system("git", "init", "-q", repo, exception: true)
    system("git", "-C", repo, "config", "user.email", "t@e.com", exception: true)
    system("git", "-C", repo, "config", "user.name", "Tester", exception: true)
    File.write(File.join(repo, "a.txt"), "hello\n")
    system("git", "-C", repo, "add", "-A", exception: true)
    system("git", "-C", repo, "commit", "-qm", "first", exception: true)
    repo
  end

  def test_sink_query_extract
    Dir.mktmpdir do |dir|
      repo = fixture_repo(dir)
      sqlite = File.join(dir, "data.sqlite")

      e = Entl.new(":memory:")
      e.sink(repo, "sqlite", sqlite)

      n = JSON.parse(e.query("SELECT count(*) AS n FROM commits")).first["n"]
      assert_equal 1, n

      snapshot = JSON.parse(e.extract("sqlite", sqlite))
      assert_equal 1, snapshot["commits"].length
      assert_equal "Tester", snapshot["commits"].first["author_name"]
    end
  end

  # The Arrow IPC stream format opens with the 0xFFFFFFFF continuation marker.
  IPC_MARKER = "\xFF\xFF\xFF\xFF".b

  def test_changes_and_query_arrow_yield_ipc_streams
    Dir.mktmpdir do |dir|
      repo = fixture_repo(dir)
      e = Entl.new(":memory:")

      tables = []
      stream = e.changes(repo, false)
      while (batch = stream.next)
        tables << batch.table
        assert_includes %w[insert update upsert delete replace], batch.op
        ipc = batch.ipc
        assert_equal Encoding::BINARY, ipc.encoding
        assert ipc.bytesize > 8, "IPC payload should be non-trivial"
        assert ipc.start_with?(IPC_MARKER), "IPC stream starts with the continuation marker"
      end
      assert_includes tables, "commits"

      ipc = e.query_arrow("SELECT 1 AS x")
      assert_equal Encoding::BINARY, ipc.encoding
      assert ipc.start_with?(IPC_MARKER)

      # Decode fully only when the (heavy, optional) red-arrow gem is around.
      begin
        require "arrow"
        decoded = Arrow::RecordBatchStreamReader.new(Arrow::Buffer.new(ipc)).read_all
        assert_equal 1, decoded.n_rows
      rescue LoadError
        # IPC marker + size assertions above are the gem-free floor.
      end
    end
  end
end
