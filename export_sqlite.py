import argparse
import datetime
import hashlib
import json
import math
from pathlib import Path
import re
import sqlite3
import sys
import uuid


ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
ULID = re.compile(r"[0-7][0-9A-HJKMNP-TV-Z]{25}\Z")


def literal(value):
    if value is None:
        return "NULL"
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("Non-finite number")
        return repr(value)
    if isinstance(value, str):
        if "\0" in value:
            raise ValueError("PostgreSQL text cannot contain NUL")
        value.encode("utf-8")
        return "E'" + value.replace("\\", "\\\\").replace("'", "''") + "'"
    raise ValueError("Unsupported SQL value")


def migrate_id(old, updated_at):
    if not isinstance(old, str):
        raise ValueError("Expected a text ID")
    if ULID.fullmatch(old):
        return old
    source = uuid.UUID(old)
    timestamp = 0
    if updated_at:
        date = datetime.datetime.fromisoformat(updated_at.replace("Z", "+00:00"))
        if date.tzinfo is None:
            raise ValueError("Timestamp must include timezone")
        timestamp = int(date.timestamp() * 1000)
    if not 0 <= timestamp < 2**48:
        raise ValueError("Timestamp outside ULID range")
    random_bits = int.from_bytes(hashlib.sha256(source.bytes).digest()[:10], "big")
    number = (timestamp << 80) | random_bits
    return "".join(ALPHABET[(number >> shift) & 31] for shift in range(125, -1, -5))


def reject_constant(value):
    raise ValueError("Non-finite JSON number")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate JSON key")
        result[key] = value
    return result


def export(source, output):
    source = Path(source).resolve(strict=True)
    output = Path(output)
    if output.exists():
        raise FileExistsError("Output already exists")
    # Open SQLite in read‑only mode and ensure the file handle is released promptly
    connection = sqlite3.connect(str(source))
    try:
        connection.execute("PRAGMA query_only = ON")
        connection.execute("BEGIN")
        tables = connection.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchall()
        columns = connection.execute('PRAGMA table_info("stores")').fetchall()
        if tables != [("stores",)] or [(c[1], c[2].upper()) for c in columns] != [("id", "TEXT"), ("data", "TEXT")]:
            raise ValueError("Unsupported SQLite schema; expected only stores(id TEXT, data TEXT)")
        rows = connection.execute('SELECT "id", "data" FROM "stores" ORDER BY rowid').fetchall()
    finally:
        connection.close()
        del connection

    mapping = {}
    converted = []
    used = set()
    for old, raw in rows:
        data = json.loads(raw, parse_constant=reject_constant, object_pairs_hook=unique_object)
        if not isinstance(data, dict) or data.get("id") != old or old in mapping:
            raise ValueError("Duplicate or inconsistent store ID")
        new = migrate_id(old, data.get("updated_at", ""))
        if new in used:
            raise ValueError("ULID collision")
        mapping[old] = new
        used.add(new)
        data["id"] = new
        converted.append((new, json.dumps(data, ensure_ascii=False, allow_nan=False, separators=(",", ":"))))
    statements = [
        "BEGIN;",
        "SET LOCAL client_encoding = 'UTF8';",
        "CREATE TABLE IF NOT EXISTS stores (id TEXT PRIMARY KEY NOT NULL, data TEXT NOT NULL, insertion_order BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE NOT NULL);",
        "LOCK TABLE stores IN ACCESS EXCLUSIVE MODE;",
        "DO $$ BEGIN IF EXISTS (SELECT 1 FROM stores) THEN RAISE EXCEPTION 'Import requires an empty stores table'; END IF; END $$;",
        "CREATE TEMP TABLE store_id_mapping (old_id TEXT PRIMARY KEY, new_id TEXT UNIQUE NOT NULL) ON COMMIT DROP;",
    ]
    for old, new in mapping.items():
        statements.append(f"INSERT INTO store_id_mapping (old_id, new_id) VALUES ({literal(old)}, {literal(new)});")
    for new, data in converted:
        statements.append(f"INSERT INTO stores (id, data) VALUES ({literal(new)}, {literal(data)});")
    statements.append("COMMIT;")
    sql = "\n".join(statements) + "\n"
    sql.encode("utf-8")
    with output.open("x", encoding="utf-8", newline="\n") as destination:
        destination.write(sql)
    return len(converted)


def main():
    parser = argparse.ArgumentParser(description="Export legacy SQLite stores to transactional PostgreSQL SQL with ULIDs")
    parser.add_argument("--source", type=Path, default=Path("distributors.db"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        count = export(args.source, args.output)
    except (OSError, ValueError, TypeError, AttributeError, sqlite3.Error, OverflowError):
        print("Export failed: check schema, IDs, JSON, timestamps and paths; output must not exist. No source data was changed.", file=sys.stderr)
        return 1
    print(f"Exported {count} stores. No database was modified; protect the SQL file as sensitive data.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
