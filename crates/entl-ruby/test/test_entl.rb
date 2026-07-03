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
end
