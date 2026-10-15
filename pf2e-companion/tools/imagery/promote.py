#!/usr/bin/env python3
"""Promote curated staging finals into the bundled content tree.

Flow per docs/imagery/DEDUCTION.md Part 4.4:
  staged --(eyeball pass on review.html)--> approved --(run)--> promoted

  promote.py status
  promote.py approve <id> [--note "..."]   # requires a staged raw
  promote.py foul <id> --note "reason"     # then edit brief + gen.py --bump <id>
  promote.py run [--only id,...] [--dry-run] [--content-root PATH]

Per approved asset:
  1. downscale the raw to the role's long-edge cap (LANCZOS; never upscale)
  2. encode WebP q≈80 (fallback PNG if the Pillow build lacks WebP)
  3. sha256 the final, copy to <content-root>/<target>
  4. wire frontmatter per the spec's `wire` block:
     adventure_frontmatter → top-level image/image_alt/image_provenance on
     the adventure .md; handout → image/image_alt on that handout's flow
     item; npc_sidecar / none → no frontmatter (session doc + conventions
     resolve those).
  5. ledger final sha/bytes in MANIFEST.json; print per-file + total delta.

Gates: run refuses unapproved or alt-less assets. Deps: stdlib + Pillow +
PyYAML (frontmatter validity gate).
"""
import argparse, datetime, hashlib, json, re, shutil, sys
from pathlib import Path

import yaml
from PIL import Image
from PIL import features as _features

ROOT = Path(__file__).resolve().parent
MANIFEST = ROOT / "MANIFEST.json"
STAGING = ROOT / "staging"
DEFAULT_CONTENT = ROOT.parents[1] / "src-tauri" / "data" / "content"
WEBP_OK = _features.check("webp")
PROV_MODEL = "nano-banana-2-1"


def load_manifest():
    return json.loads(MANIFEST.read_text())


def save_manifest(m):
    m["meta"]["updated"] = datetime.date.today().isoformat()
    MANIFEST.write_text(json.dumps(m, indent=1) + "\n")


def today():
    return datetime.date.today().isoformat()


def yq(s: str) -> str:
    """YAML double-quoted scalar (alts contain commas/colons)."""
    return '"' + s.replace("\\", "\\\\").replace('"', "'") + '"'


def style_versioned_ref(style_rel: str) -> str:
    text = (ROOT / style_rel).read_text()
    m = re.match(r"---\n(.*?)\n---\n", text, re.S)
    ver = "1"
    if m:
        vm = re.search(r"^version:\s*(\d+)\s*$", m.group(1), re.M)
        if vm:
            ver = vm.group(1)
    return f"{style_rel}@{ver}"


def inject_adventure_frontmatter(md: Path, target: str, alt: str, style_ref: str,
                                 crid: str, gen_at: str, dry: bool):
    text = md.read_text()
    m = re.match(r"^(---\n)(.*?)(\n---\n)(.*)$", text, re.S)
    if not m:
        raise SystemExit(f"{md}: no frontmatter block")
    fm = m.group(2)
    if re.search(r"^image:", fm, re.M):
        print(f"  · {md.name} already carries image: — frontmatter untouched")
        return
    block = (
        f"\nimage: {target}"
        f"\nimage_alt: {yq(alt)}"
        f"\nimage_provenance:"
        f"\n  generator: ppq"
        f"\n  model: {PROV_MODEL}"
        f"\n  style_block: {style_ref}"
        f"\n  client_request_id: {crid}"
        f"\n  generated_at: {gen_at}"
        f"\n  curated: true"
        f"\n  promoted_at: {today()}"
    )
    new_fm = fm + block
    yaml.safe_load(new_fm)  # validity gate before writing
    if not dry:
        md.write_text(m.group(1) + new_fm + m.group(3) + m.group(4))
    print(f"  · injected top-level image keys into {md.name}")


def inject_handout_frontmatter(md: Path, handout_id: str, target: str, alt: str, dry: bool):
    text = md.read_text()
    fm = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    if not fm:
        raise SystemExit(f"{md}: no frontmatter block")
    if re.search(rf"- \{{ id: {re.escape(handout_id)}, image:", fm.group(1)):
        print(f"  · handout `{handout_id}` already carries image: — untouched")
        return
    pat = re.compile(rf"(- \{{ id: {re.escape(handout_id)}),")
    if not pat.search(fm.group(1)):
        raise SystemExit(f"{md}: no handout flow item `{{ id: {handout_id}, … }}`")
    new_text, n = pat.subn(rf"\1, image: {target}, image_alt: {yq(alt)},", text, count=1)
    assert n == 1
    new_fm = re.match(r"^---\n(.*?)\n---\n", new_text, re.S).group(1)
    parsed = yaml.safe_load(new_fm)  # validity gate before writing
    hit = [h for h in parsed.get("handouts", []) if h.get("id") == handout_id]
    if not hit or not hit[0].get("image") or not hit[0].get("image_alt"):
        raise SystemExit(f"{md}: injection verify failed for handout `{handout_id}`")
    if not dry:
        md.write_text(new_text)
    print(f"  · injected image keys on handout `{handout_id}` in {md.name}")


def finalize(raw: Path, cap_px: int, out_final: Path):
    im = Image.open(raw).convert("RGB")
    if max(im.size) > cap_px:
        im.thumbnail((cap_px, cap_px), Image.LANCZOS)
    if WEBP_OK:
        im.save(out_final, "WEBP", quality=80)
    else:
        out_final = out_final.with_suffix(".png")
        im.save(out_final, "PNG", optimize=True)
    return out_final


def art_tree_bytes(content_root: Path) -> int:
    return sum(f.stat().st_size for f in content_root.rglob("*")
               if f.is_file() and "art" in f.parts)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["status", "approve", "foul", "run"])
    ap.add_argument("id", nargs="?", default="")
    ap.add_argument("--note", default="")
    ap.add_argument("--specs", default=str(ROOT / "specs" / "v1.json"))
    ap.add_argument("--only", default="")
    ap.add_argument("--content-root", default=str(DEFAULT_CONTENT))
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    manifest = load_manifest()
    specs = {a["id"]: a for a in json.loads(Path(args.specs).read_text())["assets"]}
    content_root = Path(args.content_root)

    if args.cmd == "status":
        spent = sum((r.get("cost_usd") or 0.0) for r in manifest["rows"].values())
        print(f"spend ${spent:.4f} / cap ${manifest['meta']['cap_usd']:.2f}")
        for aid, a in specs.items():
            st = manifest["assets"].get(aid, {}).get("status", a.get("status", "pending"))
            print(f"  {aid:<24} {st}")
        return

    aid = args.id
    if args.cmd in ("approve", "foul"):
        if not aid or aid not in specs:
            raise SystemExit(f"unknown asset `{aid}` (not in {args.specs})")
        spec = specs[aid]
        ver = int(spec.get("v", 1))
        raw = STAGING / f"{aid}-v{ver}.png"
        entry = manifest["assets"].get(aid, {})

        if args.cmd == "approve":
            if not raw.exists():
                raise SystemExit(f"approve: no staged raw {raw.name} — generate first")
            if not (spec.get("alt") or "").strip():
                raise SystemExit(f"approve: empty alt in spec — a11y gate")
            manifest["assets"][aid] = {
                **entry, "lens": spec.get("lens", "lewisian"),
                "current_version": ver, "status": "approved", "approved_at": today(),
                **({"approve_note": args.note} if args.note else {}),
            }
            save_manifest(manifest)
            print(f"approved {aid} v{ver}")
        else:
            manifest["assets"][aid] = {
                **entry, "lens": spec.get("lens", "lewisian"),
                "current_version": ver, "status": "fouled",
                "foul_note": args.note or "(unspecified)", "fouled_at": today(),
            }
            save_manifest(manifest)
            print(f"fouled {aid} v{ver}: {args.note or '(unspecified)'}\n"
                  f"next: edit the brief in specs/v1.json, then "
                  f"gen.py --bump {aid} && gen.py --only {aid}")
        return

    # ---- run ----
    only = {s for s in args.only.split(",") if s}
    promoted, skipped = 0, 0
    for aid, spec in specs.items():
        if only and aid not in only:
            continue
        entry = manifest["assets"].get(aid, {})
        ver = int(spec.get("v", 1))
        if entry.get("status") != "approved":
            print(f"SKIP {aid}: status {entry.get('status', spec.get('status', 'pending'))} "
                  f"!= approved", flush=True)
            skipped += 1
            continue
        if not (spec.get("alt") or "").strip():
            raise SystemExit(f"{aid}: empty alt — a11y gate, fix the spec")
        raw = STAGING / f"{aid}-v{ver}.png"
        if not raw.exists():
            print(f"SKIP {aid}: raw {raw.name} missing", flush=True)
            skipped += 1
            continue

        lens = spec.get("lens", "lewisian")
        crid = f"pf2e-{lens}-{aid}-v{ver}"
        if args.dry_run:
            print(f"DRY  {aid}: would finalize {raw.name} (cap {spec['cap_px']}px) "
                  f"-> {spec['target']}  wire={spec['wire']['kind']}")
            continue

        before = art_tree_bytes(content_root)
        final = finalize(raw, int(spec["cap_px"]), STAGING / f"{aid}-v{ver}.final.webp")
        sha = hashlib.sha256(final.read_bytes()).hexdigest()
        fbytes = final.stat().st_size

        target = spec["target"].replace(".webp", final.suffix)
        dest = content_root / target
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(final, dest)

        # Frontmatter carries the PACK-RELATIVE path (DEDUCTION §4.2); the
        # UI prefixes the lens when calling get_entity_image.
        pack_rel = target[len(lens) + 1:] if target.startswith(lens + "/") else target
        wire = spec.get("wire", {"kind": "none"})
        if wire["kind"] == "adventure_frontmatter":
            inject_adventure_frontmatter(
                content_root / wire["file"], pack_rel, spec["alt"].strip(),
                style_versioned_ref(spec.get("style", "styles/lewisian.md")),
                crid, manifest["rows"].get(crid, {}).get("generated_at", today()),
                dry=False)
        elif wire["kind"] == "handout":
            inject_handout_frontmatter(
                content_root / wire["file"], wire["handout_id"], pack_rel,
                spec["alt"].strip(), dry=False)
        # npc_sidecar / none: bytes only — session doc stamps token art by
        # convention; scene vistas resolve via the background_entity_id seam.

        if crid in manifest["rows"]:
            manifest["rows"][crid].update({
                "status": "promoted", "final_sha256": sha, "final_bytes": fbytes,
                "final_file": target, "promoted_at": today(),
            })
        manifest["assets"][aid]["status"] = "promoted"
        manifest["assets"][aid]["promoted_at"] = today()
        save_manifest(manifest)
        after = art_tree_bytes(content_root)
        promoted += 1
        print(f"PROMOTED {aid:<24} final={fbytes} B ({fbytes/1024:.0f} KiB) "
              f"sha256={sha[:12]}… -> {target}", flush=True)
        print(f"  · art tree: {before} B -> {after} B (delta {after-before:+d} B)")

    total = art_tree_bytes(content_root)
    print(f"DONE promoted={promoted} skipped={skipped} — art tree total "
          f"{total} B ({total/1024:.0f} KiB; v1 budget ≈ 1 MiB)")


if __name__ == "__main__":
    main()
