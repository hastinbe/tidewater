import unittest

import release as r


class Release(unittest.TestCase):
    def test_classify(self):
        self.assertEqual(r.classify("feat: a", ""), "feat")
        self.assertEqual(r.classify("fix(ui): a", ""), "fix")
        self.assertEqual(r.classify("perf: a", ""), "perf")
        self.assertEqual(r.classify("feat!: a", ""), "breaking")
        self.assertEqual(r.classify("refactor(core)!: a", ""), "breaking")
        self.assertEqual(r.classify("fix: a", "x\n\nBREAKING CHANGE: y"), "breaking")
        for quiet in ("docs: a", "chore: a", "ci: a", "test: a", "Merge branch 'x'", "update things"):
            self.assertIsNone(r.classify(quiet, ""))

    def test_bump(self):
        self.assertIsNone(r.bump((1, 2, 3), []))
        self.assertEqual(r.bump(None, ["fix"]), (0, 1, 0))
        self.assertEqual(r.bump((0, 1, 0), ["fix"]), (0, 1, 1))
        self.assertEqual(r.bump((0, 1, 0), ["fix", "feat"]), (0, 2, 0))
        self.assertEqual(r.bump((0, 4, 2), ["breaking", "feat"]), (0, 5, 0))
        self.assertEqual(r.bump((1, 4, 2), ["fix"]), (1, 4, 3))
        self.assertEqual(r.bump((1, 4, 2), ["feat"]), (1, 5, 0))
        self.assertEqual(r.bump((1, 4, 2), ["breaking"]), (2, 0, 0))
        self.assertEqual(r.bump((0, 9, 9), ["perf"]), (0, 9, 10))

    def test_notes(self):
        n = r.notes([("feat(ui): new thing", ""), ("fix: bug", ""), ("docs: x", ""), ("feat!: gone", "")])
        self.assertEqual(n, "## Breaking changes\n\n- gone\n\n## Features\n\n- new thing\n\n## Fixes\n\n- bug\n")
        self.assertEqual(r.notes([("docs: x", "")]), "Maintenance release.\n")


if __name__ == "__main__":
    unittest.main()
