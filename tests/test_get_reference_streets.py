#!/usr/bin/env python3
#
# Copyright (c) 2019 Miklos Vajna and contributors.
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""The test_get_reference_streets module covers the get_reference_streets module."""

import unittest
import unittest.mock

import test_config

import config
import get_reference_streets
import util


def mock_make_config() -> config.Config:
    """Creates a Config instance that has its root as /tests."""
    return config.Config("tests")


class TestMain(test_config.TestCase):
    """Tests main()."""
    def test_happy(self) -> None:
        """Tests the happy path."""
        expected = util.get_content(config.get_abspath("workdir/streets-reference-gazdagret.lst"))

        argv = ["", "gazdagret"]
        with unittest.mock.patch('sys.argv', argv):
            with unittest.mock.patch("config.make_config", mock_make_config):
                get_reference_streets.main()

        actual = util.get_content(config.get_abspath("workdir/streets-reference-gazdagret.lst"))
        self.assertEqual(actual, expected)


if __name__ == '__main__':
    unittest.main()
