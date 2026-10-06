#!/usr/bin/env python3
"""Regression tests for benchmark command parity.

Kiểm tra hồi quy tính công bằng giữa các lệnh benchmark.
"""

import unittest

import bench_v2


class BenchmarkCommandParity(unittest.TestCase):
    def test_every_package_manager_disables_lifecycle_scripts(self):
        for name, config in bench_v2.PMS.items():
            with self.subTest(package_manager=name):
                self.assertIn(
                    "--ignore-scripts",
                    config["install"],
                    f"{name} must benchmark dependency installation without scripts",
                )


if __name__ == "__main__":
    unittest.main()
