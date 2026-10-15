#!/usr/bin/env python3
"""Build the human review contact sheet (review.html) over specs + staging.

One card per asset: latest staged image (or PENDING), role/dims/cost,
alt text, the do-not checklist from the lens style block, and manifest
lineage. kirk discipline: review happens here, as a set — "siblings, not
cousins". Pure stdlib; regenerate freely.
"""
import html, json, re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / "MANIFEST.json"
STAGING = ROOT / "staging"
OUT = ROOT / "review.html"

CSS = """
body{background:#15130f;color:#d9d2c0;font-family:ui-monospace,Menlo,Consolas,monospace;margin:0;padding:24px}
h1{color:#e8dfc8;font-size:20px;margin:0 0 4px}
h2{color:#c9b548;font-size:15px;margin:26px 0 10px;border-bottom:1px solid #3a352a;padding-bottom:4px}
.sub{color:#8a8577;font-size:12px;margin-bottom:16px;line-height:1.6}
.cards{display:flex;flex-wrap:wrap;gap:16px}
.card{background:#1d1a14;border:1px solid #3a352a;border-radius:10px;padding:14px;width:360px}
.card h3{margin:0 0 6px;font-size:13px;color:#f0ead6}
img{max-width:100%;border-radius:6px;border:1px solid #3a352a;background:#0f0e0b}
.pending{color:#8a8577;font-style:italic;padding:28px 0;text-align:center;border:1px dashed #3a352a;border-radius:6px}
.meta{font-size:10.5px;color:#8a8577;margin:6px 0;line-height:1.5;word-break:break-all}
.alt{font-size:11px;color:#b3ac97;font-style:italic;margin:6px 0}
.brief{font-size:10.5px;color:#7d7768;margin:6px 0}
ul.check{font-size:10.5px;color:#a89f88;margin:6px 0;padding-left:16px;line-height:1.5}
.chip{display:inline-block;font-size:9.5px;border-radius:3px;padding:1px 6px;margin-right:4px;border:1px solid #3a352a;text-transform:uppercase;letter-spacing:.05em}
.st-pending{color:#8a8577;border-color:#8a8577}
.st-staged,.st-approved{color:#e0b341;border-color:#e0b341}
.st-promoted{color:#7fbf7f;border-color:#7fbf7f}
.st-fouled{color:#d1685e;border-color:#d1685e}
footer{margin-top:30px;font-size:11px;color:#6a6558;line-height:1.7}
"""


def section(text, name):
    m = re.search(rf"^## {re.escape(name)}\s*\n(.*?)(?=^## |\Z)", text, re.S | re.M)
    return m.group(1).strip() if m else ""


def main():
    manifest = json.loads(MANIFEST.read_text())
    spent = sum((r.get("cost_usd") or 0.0) for r in manifest.get("rows", {}).values())
    cap = manifest.get("meta", {}).get("cap_usd", 15.0)

    parts = [f"<!doctype html><meta charset=utf-8><title>imagery v1 — review sheet</title><style>{CSS}</style>"]
    parts.append("<h1>Lore imagery — review sheet</h1>")
    parts.append(
        f'<div class=sub>Batch v1 (lewisian) · spend ${spent:.4f} / cap ${cap:.2f} · '
        'Review as a SET (siblings, not cousins). Per-image gate: the lens do-not checklist on each card '
        '+ the divine-persons matrix row (DEDUCTION Part 3.6) + "affectionate, not kitsch".<br>'
        'Approve: <code>promote.py approve &lt;id&gt;</code> · '
        'Foul: <code>promote.py foul &lt;id&gt; --note "…"</code>, fix the brief, '
        '<code>gen.py --bump &lt;id&gt;</code>, regen.</div>')

    for spec_path in sorted((ROOT / "specs").glob("*.json")):
        doc = json.loads(spec_path.read_text())
        assets = doc["assets"]
        style_rel = assets[0].get("style", "styles/lewisian.md") if assets else "styles/lewisian.md"
        checklist = section((ROOT / style_rel).read_text(), "Do-Not Checklist (curation gate)")
        lis = "".join(
            f"<li>{html.escape(l.lstrip('- ').strip())}</li>"
            for l in checklist.splitlines() if l.strip().startswith("-"))

        groups = {}
        for a in assets:
            groups.setdefault(a.get("adventure", ""), []).append(a)

        parts.append(f"<h2>{html.escape(spec_path.name)} — style {html.escape(style_rel)}</h2>")
        for adventure, items in groups.items():
            if adventure:
                parts.append(f'<h2 style="border:none;color:#8a8577">adventure: {html.escape(adventure)}</h2>')
            parts.append("<div class=cards>")
            for a in items:
                aid = a["id"]
                ver = int(a.get("v", 1))
                info = manifest.get("assets", {}).get(aid, {})
                status = info.get("status", a.get("status", "pending"))
                crid = f"pf2e-{a.get('lens', 'lewisian')}-{aid}-v{ver}"
                row = manifest.get("rows", {}).get(crid, {})

                raw = STAGING / f"{aid}-v{ver}.png"
                card = [f'<div class=card><h3>{html.escape(a.get("title", aid))} '
                        f'<span class="chip st-{status}">{status}</span></h3>']
                if raw.exists():
                    card.append(f'<img src="staging/{raw.name}" alt="{html.escape(a["alt"], quote=True)}" loading=lazy>')
                else:
                    card.append('<div class=pending>PENDING — not generated</div>')
                cost = row.get("cost_usd")
                dims = row.get("src_dims", "—")
                card.append(
                    f'<div class=meta>{html.escape(aid)} · role {a["role"]} · ask {a["aspect"]} '
                    f'· cap {a["cap_px"]}px · '
                    f'{"cost $" + format(cost, ".4f") if cost is not None else "cost —"} · dims {dims}<br>'
                    f'→ <code>{html.escape(a["target"])}</code><br>'
                    f'crid <code>{html.escape(crid)}</code>'
                    + (f'<br>final sha256 <code>{row["final_sha256"][:16]}…</code> '
                       f'{row.get("final_bytes", "?")} B' if row.get("final_sha256") else "")
                    + "</div>")
                card.append(f'<div class=alt>alt: {html.escape(a["alt"])}</div>')
                card.append(f'<div class=brief>{html.escape(a["brief"])}</div>')
                if lis:
                    card.append(f"<ul class=check>{lis}</ul>")
                card.append("</div>")
                parts.append("".join(card))
            parts.append("</div>")

    parts.append("""<footer>
Images are staging raws (gitignored, regenerable via client_request_id). Promotion:
promote.py run — downscales to the role cap, encodes WebP q80, sha256-ledgers the final in
MANIFEST.json, copies it to src-tauri/data/content/&lt;lens&gt;/art/…, and injects
image / image_alt / image_provenance frontmatter. It refuses unapproved or alt-less assets.
</footer>""")
    OUT.write_text("\n".join(parts))
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
