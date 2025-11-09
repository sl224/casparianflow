from pathlib import Path
import pathspec


def test_pathspec_check():
    without_slash = Path("dir1/dir2")
    with_slash = Path("dir1/dir2/")
    mismatch = Path("alkjlkj")

    lines = [str(with_slash)]
    spec = pathspec.GitIgnoreSpec.from_lines("gitwildmatch", lines)

    assert spec.match_file(without_slash)
    assert spec.match_file(with_slash)
    assert not spec.match_file(mismatch)
