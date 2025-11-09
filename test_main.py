from pathlib import Path
import pathspec


def test_pathspec_check():
    without_slash = Path("dir1/dir2")
    with_slash = Path("dir1/dir2/")
    mismatch = Path("alkjlkj")
    h = Path("**/hello")
    hs = Path("**/hello/")
    print(h)
    print(hs)

    lines = [str(with_slash)]
    spec = pathspec.GitIgnoreSpec.from_lines("gitwildmatch", lines)

    assert spec.match_file(without_slash)
    assert spec.match_file(with_slash)
    assert not spec.match_file(mismatch)


def test_skip_usr():
    usr = "/usr"

    lines = [str(Path(usr))]
    spec = pathspec.GitIgnoreSpec.from_lines("gitwildmatch", lines)

    assert spec.match_file("/usr/bin/test")
    assert not spec.match_file("/user/bin/test")


# def test_settings():
#     settings = AppSettings()
#     print(settings.model_dump_json(indent=2))
