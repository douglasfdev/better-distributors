import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from export_sqlite import export, literal, migrate_id


class ExportTests(unittest.TestCase):
    def fixture(self, root, rows):
        source = root / "source.db"
        connection = sqlite3.connect(source)
        try:
            connection.execute("CREATE TABLE IF NOT EXISTS stores (id TEXT PRIMARY KEY NOT NULL, data TEXT NOT NULL)")
            if rows:
                connection.executemany("INSERT INTO stores VALUES (?, ?)", rows)
            connection.commit()
        finally:
            connection.close()
        return source

    def test_export(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ids = ["f0000000-0000-4000-8000-000000000001", "00000000-0000-4000-8000-000000000002"]
            data = [{"id": old, "name": "D'Água\\loja\n", "updated_at": "2024-01-01T00:00:00Z", "latitude": None, "products": [{"price": 1.25}], "categories": ["limpeza"]} for old in ids]
            source = self.fixture(root, [(old, json.dumps(row)) for old, row in zip(ids, data)])
            before = source.read_bytes()
            output = root / "export.sql"
            self.assertEqual(export(source, output), 2)
            sql = output.read_text(encoding="utf-8")
            self.assertTrue(sql.startswith("BEGIN;"))
            self.assertTrue(sql.endswith("COMMIT;\n"))
            self.assertIn("D''Água", sql)
            self.assertIn('"latitude":null', sql)
            self.assertIn('"categories":["limpeza"]', sql)
            new = [migrate_id(old, row["updated_at"]) for old, row in zip(ids, data)]
            inserts = sql[sql.index("INSERT INTO stores"):]
            self.assertLess(inserts.index(new[0]), inserts.index(new[1]))
            self.assertEqual(source.read_bytes(), before)
            with self.assertRaises(FileExistsError):
                export(source, output)
            self.assertEqual(output.read_text(encoding="utf-8"), sql)

    def test_literals(self):
        self.assertEqual(literal("a'\\b"), "E'a''\\\\b'")
        self.assertEqual(literal(None), "NULL")
        self.assertEqual(literal(True), "TRUE")
        for value in [float("nan"), float("inf"), "\0", b"bytes"]:
            with self.assertRaises(ValueError):
                literal(value)

    def test_ids_and_time(self):
        old = "00000000-0000-4000-8000-000000000001"
        new = migrate_id(old, "2024-01-01T00:00:00Z")
        self.assertRegex(new, r"^[0-7][0-9A-HJKMNP-TV-Z]{25}$")
        self.assertEqual(new, migrate_id(old, "2023-12-31T21:00:00-03:00"))
        self.assertEqual(new, migrate_id(new, ""))
        self.assertTrue(migrate_id(old, "").startswith("0000000000"))
        for timestamp in ["invalid", "2024-01-01", "1960-01-01T00:00:00Z"]:
            with self.assertRaises(ValueError):
                migrate_id(old, timestamp)
        with self.assertRaises(ValueError):
            migrate_id("123", "")

    def test_bad_data_no_output(self):
        old = "00000000-0000-4000-8000-000000000001"
        for raw in ['{"id":"wrong"}', '{"id":"x","id":"y"}', '{"price":NaN}', '[]']:
            with self.subTest(raw=raw), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = self.fixture(root, [(old, raw)])
                with self.assertRaises(ValueError):
                    export(source, root / "out.sql")
                self.assertFalse((root / "out.sql").exists())

    def test_unknown_table_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.fixture(root, [])
            connection = sqlite3.connect(source)
            try:
                connection.execute("CREATE TABLE categories (id INTEGER)")
                connection.commit()
            finally:
                connection.close()

            with self.assertRaises(ValueError):
                export(source, root / "out.sql")


if __name__ == "__main__":
    unittest.main()
