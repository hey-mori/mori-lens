#!/usr/bin/env bash
#
# Files that run through their shebang must check out with LF endings.
#
# With core.autocrlf=true, the Git for Windows default, a file with no eol rule
# checks out as CRLF. `#!/usr/bin/env bash` then names `bash\r`, and every git
# hook fails with "bash\r: No such file or directory", which blocks each commit
# and push and reads like a missing interpreter. This checks both the rule in
# .gitattributes and the files on disk, for the hooks, every *.sh, and every
# file tracked as executable.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

files="$( { git ls-files -- '.githooks/*' '*.sh'; git ls-files -s | awk '$1 == "100755" { print $4 }'; } | sort -u)"
if [[ -z "$files" ]]; then
  echo "line-endings: no executable files"
  exit 0
fi

no_rule="$(printf '%s\n' "$files" | git check-attr --stdin eol | awk -F': eol: ' '$2 != "lf" { print $1 }')"
crlf=""
while IFS= read -r f; do
  if [[ -f "$f" ]] && grep -q $'\r' "$f"; then
    crlf+="$f"$'\n'
  fi
done <<<"$files"

if [[ -n "$no_rule" ]]; then
  echo "line-endings: no 'eol=lf' rule in .gitattributes for:" >&2
  printf '%s\n' "$no_rule" | sed 's/^/  /' >&2
fi
if [[ -n "$crlf" ]]; then
  echo "line-endings: CRLF on disk in:" >&2
  printf '%s' "$crlf" | sed 's/^/  /' >&2
  echo "  check each one out again: rm <file> && git checkout -- <file>" >&2
fi
if [[ -n "$no_rule" || -n "$crlf" ]]; then
  exit 1
fi
echo "line-endings: $(printf '%s\n' "$files" | wc -l | tr -d ' ') executable files keep LF"
