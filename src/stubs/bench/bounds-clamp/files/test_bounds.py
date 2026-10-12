import unittest

import bounds


class ClampTest(unittest.TestCase):
    def test_in_range(self):
        self.assertEqual(bounds.clamp(5, 0, 10), 5)

    def test_below(self):
        self.assertEqual(bounds.clamp(-1, 0, 10), 0)

    def test_above(self):
        self.assertEqual(bounds.clamp(11, 0, 10), 10)


if __name__ == "__main__":
    unittest.main()
