#!/usr/bin/env python3
"""One-off MySQL -> SQLite translator for RuoYi-Vue-Plus ry_vue_5.X.sql.

Keeps only the core system-module tables (see KEEP_TABLES) and prunes
menus / role-menu links to the ids the Rust backend actually implements
(see KEEP_MENU_IDS). Run from the repo root:

    python3 tools/mysql2sqlite.py \
        script/sql/ry_vue_5.X.sql ruoyi-rs/sql/schema.sql ruoyi-rs/sql/seed.sql
"""
import re
import sys

KEEP_TABLES = {
    "sys_dept", "sys_user", "sys_post", "sys_role", "sys_menu",
    "sys_user_role", "sys_role_menu", "sys_role_dept", "sys_user_post",
    "sys_oper_log", "sys_dict_type", "sys_dict_data", "sys_config",
    "sys_logininfor", "sys_notice", "sys_tenant", "sys_client",
}

# Menus implemented by the Rust backend (system + monitor/log management).
KEEP_MENU_IDS = set(range(1001, 1049)) | {1050}
KEEP_MENU_IDS |= {1, 2, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 500, 501, 130, 131, 132}

TYPE_MAP = [
    (re.compile(r"bigint\(\d+\)|tinyint\(\d+\)|int\(\d+\)|int\b"), "INTEGER"),
    (re.compile(r"varchar\(\d+\)"), "TEXT"),
    (re.compile(r"char\(\d+\)"), "TEXT"),
    (re.compile(r"datetime(\(\d+\))?"), "TEXT"),
    (re.compile(r"longblob|blob"), "TEXT"),
    (re.compile(r"\btext\b"), "TEXT"),
]


def map_type(col: str) -> str:
    for pat, repl in TYPE_MAP:
        col = pat.sub(repl, col, count=1)
    return col


def split_statements(sql: str):
    """Split into statements respecting single-quoted strings."""
    stmts, cur, in_str, i = [], [], False, 0
    while i < len(sql):
        ch = sql[i]
        cur.append(ch)
        if ch == "'":
            in_str = not in_str
        elif ch == ";" and not in_str:
            stmts.append("".join(cur).strip())
            cur = []
        i += 1
    tail = "".join(cur).strip()
    if tail:
        stmts.append(tail)
    return stmts


def convert_ddl(stmt: str):
    m = re.match(r"create\s+table\s+(\w+)", stmt, re.I)
    if not m:
        return None
    table = m.group(1)
    if table not in KEEP_TABLES:
        return None
    body = stmt[stmt.index("(") + 1: stmt.rindex(")")]
    lines = [l.strip() for l in body.split("\n") if l.strip()]
    cols, pk, uniqs = [], None, []
    for line in lines:
        line = line.rstrip(",")
        lm = re.match(r"(?i)key\s+\w+\s*\(", line)
        if lm:  # plain index -> drop (SQLite auto-indexes PK/UNIQUE)
            continue
        pm = re.match(r"(?i)primary\s+key\s*\((.+)\)", line)
        if pm:
            pk = f"PRIMARY KEY ({pm.group(1)})"
            continue
        um = re.match(r"(?i)unique\s*\((.+)\)", line)
        if um:
            uniqs.append(f"UNIQUE ({um.group(1)})")
            continue
        line = re.sub(r"\s*comment\s*'[^']*'", "", line, flags=re.I)
        parts = line.split(None, 1)
        if len(parts) != 2:
            continue
        name, rest = parts
        if name.lower() in ("constraint", "index", "key", "primary", "unique", "fulltext"):
            continue
        rest = map_type(rest)
        rest = re.sub(r"\bdefault\s+current_timestamp\b", "", rest, flags=re.I)
        cols.append(f"    {name} {rest.strip()}")
    defs = cols + ([f"    {pk}"] if pk else []) + [f"    {u}" for u in uniqs]
    return f"CREATE TABLE {table} (\n" + ",\n".join(defs) + "\n);"


def convert_insert(stmt: str):
    m = re.match(r"insert\s+into\s+(\w+)\s+values", stmt, re.I)
    if not m:
        return None
    table = m.group(1)
    if table not in KEEP_TABLES:
        return None
    s = re.sub(r"(?i)insert\s+into\s+(\w+)", r"INSERT INTO \1", stmt)
    s = re.sub(r"\bsysdate\s*\(\s*\)", "datetime('now')", s, flags=re.I)
    s = s.rstrip().rstrip(";") + ";"
    if table == "sys_menu":
        mid = re.search(r"values\s*\(\s*'?(\d+)'?", s, re.I)
        if not mid or int(mid.group(1)) not in KEEP_MENU_IDS:
            return None
    if table == "sys_role_menu":
        mid = re.search(r"values\s*\(\s*'?(\d+)'?\s*,\s*'?(\d+)'?", s, re.I)
        if not mid or int(mid.group(2)) not in KEEP_MENU_IDS:
            return None
    return s


def main():
    src, schema_out, seed_out = sys.argv[1], sys.argv[2], sys.argv[3]
    sql = open(src, encoding="utf-8").read()
    sql = re.sub(r"--[^\n]*", "", sql)  # line comments are safe (no '--' inside strings here)
    ddls, seeds = [], []
    for stmt in split_statements(sql):
        low = stmt.lower().lstrip()
        if low.startswith("create table"):
            ddl = convert_ddl(stmt)
            if ddl:
                ddls.append(ddl)
        elif low.startswith("insert into"):
            ins = convert_insert(stmt)
            if ins:
                seeds.append(ins)
    # role 4's seeded menus are all demo-module menus which this port does not
    # implement; give it role 3's (system+monitor) set instead so the 仅本人
    # data-scope demo account stays usable.
    role3_menus = sorted({
        int(m.group(1))
        for s in seeds
        if "sys_role_menu" in s
        for m in [re.search(r"\('3',\s*'?(\d+)'?\)", s)]
        if m
    })
    seeds = [s for s in seeds if not ("sys_role_menu" in s and re.search(r"values\s*\(\s*'4'", s, re.I))]
    seeds += [f"INSERT INTO sys_role_menu values ('4', '{m}');" for m in role3_menus]
    open(schema_out, "w", encoding="utf-8").write(
        "-- SQLite schema translated from script/sql/ry_vue_5.X.sql (core system tables only)\n"
        "PRAGMA journal_mode=WAL;\n\n" + "\n\n".join(ddls) + "\n"
    )
    open(seed_out, "w", encoding="utf-8").write(
        "-- Seed data translated from script/sql/ry_vue_5.X.sql (pruned to implemented menus)\n"
        + "\n".join(seeds) + "\n"
    )
    print(f"tables: {len(ddls)}, seed statements: {len(seeds)}")
    for d in ddls:
        print(" ", d.split("\n")[0])


if __name__ == "__main__":
    main()
