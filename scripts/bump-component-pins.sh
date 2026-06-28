#!/usr/bin/env bash
# Hebt die bridge+trace-Pins in release.yml auf das jeweils NEUESTE Tag der Komponenten-Repos.
# So ist "Sonar-Release" garantiert das neueste bridge+trace+forge-Paket (forge-control = in-repo).
# Danach: Sonar-Version bumpen + taggen. Der Drift-Guard im Release erzwingt das ohnehin.
set -euo pipefail
cd "$(dirname "$0")/.."
RY=".github/workflows/release.yml"
PROJ="$HOME/subunit/unitone/workspace/projects"
bump() {  # $1=GitHub-Repo  $2=lokaler-Klon-Name
  cd "$PROJ/$2"; git fetch origin --tags --quiet
  local latest sha
  latest=$(git tag --sort=-v:refname | grep -E "^v[0-9]" | head -1)
  sha=$(git rev-list -n1 "$latest")
  cd "$OLDPWD"
  python3 - "$RY" "$1" "$sha" "$latest" <<'PY'
import re,sys
ry,repo,sha,latest=sys.argv[1:5]; s=open(ry).read()
pat=re.compile(r"(repository: subunit-ai/"+re.escape(repo)+r"\n(?:\s+[^\n]*\n)*?\s+ref: )[a-f0-9]{40}( #[^\n]*)?")
s2,n=pat.subn(lambda m: m.group(1)+sha+f" # {latest} (auto-bump bump-component-pins.sh)", s, count=1)
assert n==1, f"{repo}: ref-Zeile nicht gefunden"
open(ry,"w").write(s2); print(f"  {repo} -> {latest} ({sha[:10]})")
PY
}
echo "Bumpe Pins auf neueste Tags:"
bump bridge-tauri subunit-bridge
bump trace-tauri trace-tauri
echo "Fertig. Naechster Schritt: Sonar-Version in tauri.conf.json bumpen, commit + tag."
