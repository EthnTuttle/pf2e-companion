#!/usr/bin/env python3
"""pf2e-companion lore-imagery generator (kirk artbake shape, no quant).

Reads specs/*.json, assembles style-block prefix + brief + suffix, calls
PayPerQ (OpenAI-images-shaped), writes staging/<id>-vN.png (+ the assembled
prompt beside it for drift review), and appends ledger rows to MANIFEST.json.

Discipline (from docs/imagery/DEDUCTION.md):
- PPQ_API_KEY env ONLY. Never written to disk, never logged.
- client_request_id = pf2e-<lens>-<asset>-vN (idempotency; retries attach
  to finished generations and never double-bill). Bump N via --bump <id>
  when a brief changes materially.
- Budget cap enforced here: cumulative manifest cost >= cap -> refuse;
  >= 80% of cap -> stop the batch and report (human decision 2026-10-14).
- Raws are staging-local (gitignored), regenerable via client_request_id.
- Deps: python3 stdlib only.

Usage:
  gen.py --dry-run                 # assemble + print prompts, no network
  gen.py [--specs specs/v1.json] [--only id,id2]
  gen.py --bump <id>               # retire the current client_request_id
"""
import argparse, hashlib, json, os, re, struct, sys, urllib.error, urllib.request
from datetime import date
from pathlib import Path

API = "https://api.ppq.ai/v1/images/generations"
KEY = os.environ.get("PPQ_API_KEY", "")
ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / "MANIFEST.json"
STAGING = ROOT / "staging"

STOP_FRACTION = 0.8  # stop and report once cumulative spend reaches 80% of cap


def load_manifest():
    return json.loads(MANIFEST.read_text())


def save_manifest(m):
    m["meta"]["updated"] = date.today().isoformat()
    MANIFEST.write_text(json.dumps(m, indent=1) + "\n")


def spent_usd(manifest):
    return sum((r.get("cost_usd") or 0.0) for r in manifest["rows"].values())


def load_specs(path):
    return json.loads(Path(path).read_text())["assets"]


def style_block(style_rel):
    """styles/<lens>.md -> (prefix, suffix, versioned_ref)."""
    text = (ROOT / style_rel).read_text()
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    ver = "1"
    if m:
        vm = re.search(r"^version:\s*(\d+)\s*$", m.group(1), re.M)
        if vm:
            ver = vm.group(1)

    def section(name):
        s = re.search(rf"^## {re.escape(name)}\s*\n(.*?)(?=^## |\Z)", text, re.S | re.M)
        return s.group(1).strip() if s else ""

    prefix, suffix = section("Prefix"), section("Suffix")
    if not prefix or not suffix:
        raise SystemExit(f"style {style_rel} missing ## Prefix / ## Suffix sections")
    return prefix, suffix, f"{style_rel}@{ver}"


def ppq_generate(model, prompt, size, crid):
    payload = {
        "model": model,
        "prompt": prompt,
        "n": 1,
        "size": size,
        "output_format": "png",
        "client_request_id": crid,
    }
    req = urllib.request.Request(
        API,
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=290) as r:
        body = json.load(r)
    with urllib.request.urlopen(body["data"][0]["url"], timeout=120) as r:
        png = r.read()
    return png, body.get("cost")


def png_dims(data: bytes):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    w, h = struct.unpack(">II", data[16:24])
    return w, h


def bump(specs_path, asset_id):
    doc = json.loads(Path(specs_path).read_text())
    for a in doc["assets"]:
        if a["id"] == asset_id:
            a["v"] = int(a.get("v", 1)) + 1
            a["status"] = "pending"
            Path(specs_path).write_text(json.dumps(doc, indent=2) + "\n")
            print(f"bumped {asset_id}: -> v{a['v']} (client_request_id retires the old cell; "
                  f"edit the brief in the same change if the foul was wording)")
            return
    raise SystemExit(f"--bump: no asset `{asset_id}` in {specs_path}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--specs", default=str(ROOT / "specs" / "v1.json"))
    ap.add_argument("--only", default="")
    ap.add_argument("--max-spend", type=float, default=None)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--bump", default="")
    args = ap.parse_args()

    if args.bump:
        bump(args.specs, args.bump)
        return

    assets = load_specs(args.specs)
    manifest = load_manifest()
    cap = (args.max_spend if args.max_spend is not None
           else float(manifest["meta"]["cap_usd"]))
    model = manifest["meta"]["model"]  # single-provider decision (2026-10-14)
    only = {s for s in args.only.split(",") if s}

    spent = spent_usd(manifest)
    if spent >= cap:
        print(f"REFUSE: cumulative spend ${spent:.4f} >= cap ${cap:.2f}. "
              f"Budget raises are a human decision.")
        sys.exit(2)

    results, failures = {}, {}
    for a in [x for x in assets if not only or x["id"] in only]:
        aid, ver = a["id"], int(a.get("v", 1))
        lens = a.get("lens", "lewisian")
        crid = f"pf2e-{lens}-{aid}-v{ver}"
        raw = STAGING / f"{aid}-v{ver}.png"
        prefix, suffix, style_ref = style_block(a.get("style", "styles/lewisian.md"))
        prompt = f"{prefix} {a['brief'].strip()} {suffix}"

        if raw.exists():
            print(f"SKIP {aid} v{ver} (raw exists)", flush=True)
            continue
        if not args.dry_run and not KEY:
            print("STOP: PPQ_API_KEY absent from env. Export it and rerun:\n"
                  "  export PPQ_API_KEY=sk-…   # password store only; never commit\n"
                  f"  tools/imagery/.venv/bin/python tools/imagery/gen.py --specs {args.specs}")
            sys.exit(3)
        if spent >= STOP_FRACTION * cap:
            print(f"STOP: cumulative spend ${spent:.4f} reached 80% of the ${cap:.2f} cap "
                  f"— halting the batch and reporting (human gate).", flush=True)
            break

        if args.dry_run:
            print(f"--- {aid} v{ver} [{a['role']} {a['aspect']} cap {a['cap_px']}px] crid={crid}"
                  f"\n{prompt}\n", flush=True)
            continue

        STAGING.mkdir(exist_ok=True)
        try:
            try:
                png, cost = ppq_generate(model, prompt, a["aspect"], crid)
                size_used = a["aspect"]
                note = None
            except urllib.error.HTTPError as e:
                # kirk ART-STATUS §2: one spurious-400 lane — if the aspect
                # string was rejected, fall back to 1:1 once. Never retry-spam.
                if e.code == 400 and a["aspect"] != "1:1":
                    png, cost = ppq_generate(model, prompt, "1:1", crid)
                    size_used = "1:1"
                    note = f"aspect {a['aspect']} rejected (HTTP 400); fell back to 1:1"
                else:
                    raise
            raw.write_bytes(png)
            (STAGING / f"{aid}-v{ver}.prompt.txt").write_text(prompt + "\n")
            dims = png_dims(png)
            cost = float(cost or 0.0)
            row = {
                "asset": aid,
                "version": ver,
                "client_request_id": crid,
                "model": f"ppq:{model}",
                "style_block": style_ref,
                "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
                "size_requested": size_used,
                "src_dims": f"{dims[0]}x{dims[1]}" if dims else "?",
                "raw_bytes": len(png),
                "cost_usd": cost,
                "status": "staged",
                "generated_at": date.today().isoformat(),
            }
            if note:
                row["note"] = note
            manifest["rows"][crid] = row
            manifest["assets"][aid] = {
                "current_version": ver,
                "lens": lens,
                "role": a["role"],
                "target": a["target"],
                "alt": a["alt"].strip(),
                "status": "staged",
            }
            save_manifest(manifest)
            spent += cost
            results[aid] = {"cost": cost, "dims": row["src_dims"]}
            print(f"OK   {aid:<24} {row['src_dims']:>10} cost=${cost:.4f} "
                  f"(cumulative ${spent:.4f}/${cap:.2f})", flush=True)
        except urllib.error.HTTPError as e:
            failures[aid] = f"HTTP {e.code}: {e.read()[:200]!r}"
            print(f"FAIL {aid:<24} HTTP {e.code} (if moderation: reword, don't retry-spam)",
                  flush=True)
        except Exception as e:
            failures[aid] = f"{type(e).__name__}: {e}"
            print(f"FAIL {aid:<24} {type(e).__name__}: {str(e)[:80]}", flush=True)

    if not args.dry_run:
        save_manifest(manifest)
    total = spent_usd(manifest)
    print(f"DONE ok={len(results)} fail={len(failures)} total_spend=${total:.4f} / cap=${cap:.2f}")
    for k, v in failures.items():
        print(f"  fail {k}: {v}")


if __name__ == "__main__":
    main()
