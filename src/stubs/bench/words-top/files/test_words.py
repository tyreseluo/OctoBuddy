import unittest

import words


class TopWordsTest(unittest.TestCase):
    def test_picks_the_most_frequent(self):
        self.assertEqual(words.top_words("a b b c c c", 1), ["c"])

    def test_case_insensitive(self):
        self.assertEqual(words.top_words("Apple apple APPLE banana", 1), ["apple"])

    def test_fewer_words_than_n(self):
        self.assertEqual(words.top_words("x y", 5), ["x", "y"])


if __name__ == "__main__":
    unittest.main()
