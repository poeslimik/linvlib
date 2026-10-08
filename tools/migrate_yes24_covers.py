#!/usr/bin/env python3
"""Re-fetch Yes24 metadata (covers) for volumes in a linvlib SQLite DB.

Stops when the soft daily Yes24 quota would be exceeded.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

YES24_DETAIL = "https://apis.yes24.com/v1/goods/itemDetail"
RPS_INTERVAL = 0.11  # ~9 req/s under Basic 10 RPS
DAILY_SOFT = 19_500
DAILY_HARD = 20_000


def load_dotenv(path: Path) -> None:
    if not path.is_file():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        val = val.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = val


def ensure_quota_table(conn: sqlite3.Connection) -> None:
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS yes24_api_usage (
            usage_date TEXT PRIMARY KEY NOT NULL,
            query_count INTEGER NOT NULL DEFAULT 0
        )
        """
    )
    # Migrate rename if old table still present
    row = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='aladin_api_usage'"
    ).fetchone()
    if row:
        exists = conn.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='yes24_api_usage'"
        ).fetchone()
        if not exists:
            conn.execute("ALTER TABLE aladin_api_usage RENAME TO yes24_api_usage")
        else:
            # Merge counts then drop old
            for d, c in conn.execute(
                "SELECT usage_date, query_count FROM aladin_api_usage"
            ):
                conn.execute(
                    """
                    INSERT INTO yes24_api_usage (usage_date, query_count) VALUES (?, ?)
                    ON CONFLICT(usage_date) DO UPDATE SET
                      query_count = MAX(yes24_api_usage.query_count, excluded.query_count)
                    """,
                    (d, c),
                )
            conn.execute("DROP TABLE aladin_api_usage")
    conn.commit()


def seoul_today() -> str:
    # KST = UTC+9
    return time.strftime("%Y-%m-%d", time.gmtime(time.time() + 9 * 3600))


def quota_used(conn: sqlite3.Connection, day: str) -> int:
    row = conn.execute(
        "SELECT query_count FROM yes24_api_usage WHERE usage_date = ?", (day,)
    ).fetchone()
    return int(row[0]) if row else 0


def consume_quota(conn: sqlite3.Connection, day: str, soft: int) -> bool:
    ensure_quota_table(conn)
    conn.execute(
        "INSERT INTO yes24_api_usage (usage_date, query_count) VALUES (?, 0) "
        "ON CONFLICT(usage_date) DO NOTHING",
        (day,),
    )
    used = quota_used(conn, day)
    if used + 1 > soft:
        conn.commit()
        return False
    conn.execute(
        "UPDATE yes24_api_usage SET query_count = query_count + 1 WHERE usage_date = ?",
        (day,),
    )
    conn.commit()
    return True


def yes24_by_isbn(api_key: str, isbn13: str) -> dict | None:
    qs = urllib.parse.urlencode(
        {"searchType": "ISBN13", "query": isbn13, "detail": "N"}
    )
    req = urllib.request.Request(
        f"{YES24_DETAIL}?{qs}",
        headers={"X-Api-Key": api_key, "Accept": "application/json"},
        method="GET",
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")
        if e.code == 404:
            return None
        raise RuntimeError(f"Yes24 HTTP {e.code}: {body[:300]}") from e
    if not data.get("success"):
        code = data.get("errorCode") or ""
        if code in ("GOODS_001", "GOODS_002", "SEARCH_001"):
            return None
        raise RuntimeError(f"Yes24 API error: {data.get('message')} ({code})")
    items = (data.get("data") or {}).get("items") or []
    return items[0] if items else None


def looks_aladin(url: str | None) -> bool:
    if not url:
        return True
    u = url.lower()
    return "aladin" in u or "image.aladin" in u or "cover.aladin" in u


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--src",
        default=r"c:\Users\iskim\Downloads\linvlib-20260924-000033.db",
    )
    parser.add_argument(
        "--dst",
        default="",
        help="Working copy path (default: src with -yes24 suffix beside it)",
    )
    parser.add_argument("--soft-limit", type=int, default=DAILY_SOFT)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--resume",
        action="store_true",
        help="Do not re-copy; continue on --dst (default if dst already exists)",
    )
    parser.add_argument("--force-all-covers", action="store_true",
                        help="Replace all covers, not only Aladin hosts")
    parser.add_argument(
        "--series-covers-only",
        action="store_true",
        help="No API calls; set series.cover_url from latest Yes24 volume cover when Aladin/empty",
    )
    args = parser.parse_args()

    if args.series_covers_only:
        dst = Path(args.dst) if args.dst else Path(args.src)
        conn = sqlite3.connect(str(dst))
        n = 0
        for (sid,) in conn.execute("SELECT id FROM series"):
            row = conn.execute(
                "SELECT cover_url FROM series WHERE id = ?", (sid,)
            ).fetchone()
            cur = (row[0] or "").strip() if row else ""
            if cur and not looks_aladin(cur):
                continue
            vol = conn.execute(
                """
                SELECT cover_url FROM volumes
                WHERE series_id = ?
                  AND cover_url IS NOT NULL AND TRIM(cover_url) != ''
                  AND lower(cover_url) LIKE '%yes24%'
                ORDER BY volume_number DESC
                LIMIT 1
                """,
                (sid,),
            ).fetchone()
            if not vol:
                continue
            conn.execute(
                "UPDATE series SET cover_url = ? WHERE id = ?", (vol[0], sid)
            )
            n += 1
        conn.commit()
        conn.close()
        print(f"series covers refreshed from yes24 volumes: {n}")
        return 0

    repo = Path(__file__).resolve().parents[1]
    load_dotenv(repo / ".env")
    api_key = os.environ.get("YES24_API_KEY", "").strip()
    if not api_key:
        print("YES24_API_KEY is not set", file=sys.stderr)
        return 2

    src = Path(args.src)
    if not src.is_file():
        print(f"source DB not found: {src}", file=sys.stderr)
        return 2

    if args.dst:
        dst = Path(args.dst)
    else:
        dst = src.with_name(src.stem + "-yes24" + src.suffix)

    resume = args.resume or (dst.is_file() and dst.resolve() != src.resolve())
    if args.dry_run:
        dst = src
    elif resume and dst.is_file():
        print(f"resume on existing {dst}")
    else:
        print(f"copy {src} -> {dst}")
        shutil.copy2(src, dst)

    conn = sqlite3.connect(str(dst))
    conn.row_factory = sqlite3.Row
    ensure_quota_table(conn)

    day = seoul_today()
    used0 = quota_used(conn, day)
    print(f"quota today {day}: {used0}/{args.soft_limit} (hard {DAILY_HARD})")

    vols = conn.execute(
        """
        SELECT id, series_id, title, cover_url, isbn13, aladin_item_id
        FROM volumes
        WHERE isbn13 IS NOT NULL AND TRIM(isbn13) != ''
        ORDER BY series_id, volume_number, id
        """
    ).fetchall()
    print(f"volumes with ISBN: {len(vols)}")

    # Prefer unique ISBNs to save quota; apply result to all volumes with that ISBN
    by_isbn: dict[str, list[sqlite3.Row]] = {}
    for v in vols:
        isbn = "".join(ch for ch in (v["isbn13"] or "") if ch.isdigit())
        if len(isbn) != 13:
            continue
        by_isbn.setdefault(isbn, []).append(v)
    print(f"unique ISBN13: {len(by_isbn)}")

    updated_vols = 0
    updated_series = 0
    missing = 0
    skipped_cover = 0
    calls = 0
    stopped_quota = False
    last_call = 0.0
    series_cover_touch: set[str] = set()

    for isbn, group in by_isbn.items():
        # Skip if every volume already has a non-Aladin cover (unless force)
        if not args.force_all_covers:
            need = [v for v in group if looks_aladin(v["cover_url"])]
            if not need:
                skipped_cover += len(group)
                continue
        else:
            need = list(group)

        if used0 + calls + 1 > args.soft_limit:
            stopped_quota = True
            print(f"soft quota would exceed; stopping before ISBN {isbn}")
            break

        # RPS pacing
        elapsed = time.monotonic() - last_call
        if elapsed < RPS_INTERVAL:
            time.sleep(RPS_INTERVAL - elapsed)

        if args.dry_run:
            calls += 1
            last_call = time.monotonic()
            continue

        if not consume_quota(conn, day, args.soft_limit):
            stopped_quota = True
            print(f"soft quota reached after {calls} calls; stopping")
            break

        try:
            item = yes24_by_isbn(api_key, isbn)
        except Exception as e:
            print(f"ERROR isbn={isbn}: {e}", file=sys.stderr)
            # quota already consumed; continue carefully
            calls += 1
            last_call = time.monotonic()
            continue

        calls += 1
        last_call = time.monotonic()

        if not item:
            missing += 1
            print(f"  miss {isbn}")
            continue

        cover = (item.get("cover") or "").strip()
        item_id = item.get("itemId")
        if not cover:
            missing += 1
            print(f"  no cover {isbn} itemId={item_id}")
            continue

        new_key = f"isbn:{isbn}"
        for v in need:
            # Cover always. External id only when empty/numeric and not already taken.
            conn.execute(
                "UPDATE volumes SET cover_url = ? WHERE id = ?",
                (cover, v["id"]),
            )
            updated_vols += 1
            cur_key = (v["aladin_item_id"] or "").strip()
            if (not cur_key) or cur_key.isdigit():
                taken = conn.execute(
                    "SELECT 1 FROM volumes WHERE aladin_item_id = ? AND id != ? LIMIT 1",
                    (new_key, v["id"]),
                ).fetchone()
                if not taken:
                    conn.execute(
                        "UPDATE volumes SET aladin_item_id = ? WHERE id = ?",
                        (new_key, v["id"]),
                    )
            series_cover_touch.add(v["series_id"])

        if calls % 50 == 0:
            conn.commit()
            print(
                f"… calls={calls} updated_vols={updated_vols} missing={missing} "
                f"quota={used0 + calls}/{args.soft_limit}"
            )

    # Refresh series.cover_url from latest volume cover when Aladin/empty
    for sid in series_cover_touch:
        row = conn.execute(
            "SELECT cover_url FROM series WHERE id = ?", (sid,)
        ).fetchone()
        cur_cover = row["cover_url"] if row else None
        if args.force_all_covers or looks_aladin(cur_cover):
            vol = conn.execute(
                """
                SELECT cover_url FROM volumes
                WHERE series_id = ?
                  AND cover_url IS NOT NULL AND TRIM(cover_url) != ''
                ORDER BY volume_number DESC, published_at DESC
                LIMIT 1
                """,
                (sid,),
            ).fetchone()
            if vol and vol["cover_url"]:
                conn.execute(
                    "UPDATE series SET cover_url = ? WHERE id = ?",
                    (vol["cover_url"], sid),
                )
                updated_series += 1

    conn.commit()
    used1 = quota_used(conn, day)
    conn.close()

    print("---")
    print(f"dst: {dst}")
    print(f"api_calls: {calls}")
    print(f"updated_volumes: {updated_vols}")
    print(f"updated_series_covers: {updated_series}")
    print(f"missing_yes24: {missing}")
    print(f"skipped_already_ok: {skipped_cover}")
    print(f"quota_now: {used1}/{args.soft_limit}")
    print(f"stopped_for_quota: {stopped_quota}")
    return 0 if not stopped_quota or updated_vols > 0 else 0


if __name__ == "__main__":
    raise SystemExit(main())
