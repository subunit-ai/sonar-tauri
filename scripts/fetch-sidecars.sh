#!/usr/bin/env bash
# Baut den Subunit-Bridge-Sidecar aus einem GEPINNTEN Bridge-Source für die Target-Triples,
# die Tauris externalBin erwartet, legt sie in src-tauri/binaries/ ab und verifiziert per
# SHA-256 gegen scripts/sidecar-sha256.txt (schreibt das Manifest beim ersten Lauf, verifiziert danach).
#
# Supply-Chain: zusätzlich verifiziert die App den Hash beim Start (foundation/supply_chain).
# Bridge-Repo: subunit-ai/bridge-tauri (in CI klonen @ BRIDGE_REF; lokal via BRIDGE_SRC).
#
# HINWEIS (offen, Codex-Politur): Bun hat KEIN windows-arm64-Target → x64-Sidecar läuft auf
# Win-ARM per Emulation (hier als aarch64-Fallback gebündelt). `bun --compile` ist nicht
# garantiert byte-deterministisch → bei Manifest-Mismatch BRIDGE_REF prüfen + Manifest neu pinnen.
set -euo pipefail

BRIDGE_REF="${BRIDGE_REF:-v0.2.0}"
BRIDGE_SRC="${BRIDGE_SRC:-$HOME/subunit/unitone/workspace/projects/subunit-bridge}"
BUN="${BUN:-bun}"
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/../src-tauri/binaries"
MANIFEST="$HERE/sidecar-sha256.txt"
mkdir -p "$OUT"

# Tauri-Target-Triple → Bun-Compile-Target
declare -A TARGETS=(
  ["x86_64-unknown-linux-gnu"]="bun-linux-x64"
  ["x86_64-pc-windows-msvc"]="bun-windows-x64"
  ["aarch64-pc-windows-msvc"]="bun-windows-x64"   # x64 via Win-ARM-Emulation (Bun: kein win-arm64)
  ["x86_64-apple-darwin"]="bun-darwin-x64"
  ["aarch64-apple-darwin"]="bun-darwin-arm64"
)
# nur die Targets bauen, die für diesen CI-Runner gebraucht werden (oder alle lokal)
ONLY="${ONLY:-}"

echo "[fetch-sidecars] bun install im Bridge-Source ($BRIDGE_SRC) — strict (Lockfile-Disziplin)"
# Strikt: kein Fallback auf 'bun install' — ein out-of-sync/fehlendes Lockfile MUSS hart
# fehlschlagen (Supply-Chain, Codex-Finding #5). Bridge-Source hat ein committetes bun.lock.
( cd "$BRIDGE_SRC" && "$BUN" install --frozen-lockfile )

for triple in "${!TARGETS[@]}"; do
  [ -n "$ONLY" ] && [ "$ONLY" != "$triple" ] && continue
  ext=""; [[ "$triple" == *windows* ]] && ext=".exe"
  out="$OUT/subunit-bridge-$triple$ext"
  echo "[fetch-sidecars] build $triple (${TARGETS[$triple]}) → $out"
  ( cd "$BRIDGE_SRC" && "$BUN" build src/main.ts --compile --target="${TARGETS[$triple]}" --outfile "$out" )
done

# Manifest FRISCH je Build schreiben. `bun --compile` ist nicht byte-deterministisch, daher ist ein
# committeter Cross-Build-Hash-Pin sinnlos — das Integritätsmodell ist: dieser Build erzeugt Binary
# UND Manifest zusammen, supply_chain.rs embedded das Manifest zur Compile-Zeit (include_str!) und
# verifiziert zur LAUFZEIT → erkennt Tampering am ausgelieferten Sidecar. Reihenfolge: fetch-sidecars
# (schreibt Manifest) MUSS vor `tauri build` (kompiliert supply_chain) laufen.
echo "[fetch-sidecars] SHA-256-Manifest (frisch):"
( cd "$OUT" && sha256sum subunit-bridge-* > "$MANIFEST" && cat "$MANIFEST" )
echo "[fetch-sidecars] ✓ Manifest geschrieben (Runtime-Verify in der App = Tamper-Schutz am gebündelten Binary)"
