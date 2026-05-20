#!/usr/bin/env bash
# Generate Terraform _locals.tf files from shared/constants/src/lib.rs.
# Rust is the canonical source for shared constants — do not edit _locals.tf by hand.
set -euo pipefail

RUST_SRC="shared/constants/src/lib.rs"
TF_OUT=(
  "terraform/modules/archive_it/_locals.tf"
  "terraform/modules/stack/_locals.tf"
  "terraform/modules/users/_locals.tf"
)

generate_locals() {
  local out="$1"

  {
    cat <<'HEADER'
# Generated from shared/constants/src/lib.rs — do not edit.
# Run `make locals` to regenerate.

locals {
HEADER

    while IFS= read -r line; do
      # Pass through section comments
      if [[ $line =~ ^//\ (.+) ]]; then
        echo ""
        echo "  # ${BASH_REMATCH[1]}"

      # pub const NAME: &str = "value";
      elif [[ $line =~ ^pub\ const\ ([A-Z_]+):\ \&str\ =\ \"([^\"]*)\"\; ]]; then
        name="$(echo "${BASH_REMATCH[1]}" | tr '[:upper:]' '[:lower:]')"
        value="${BASH_REMATCH[2]}"
        printf '  %-45s = "%s"\n' "$name" "$value"

      # pub const NAME: <integer> = <number>;
      elif [[ $line =~ ^pub\ const\ ([A-Z_]+):\ (u8|u16|u32|u64|usize|i8|i16|i32|i64)\ =\ ([0-9]+)\; ]]; then
        name="$(echo "${BASH_REMATCH[1]}" | tr '[:upper:]' '[:lower:]')"
        value="${BASH_REMATCH[3]}"
        printf '  %-45s = %s\n' "$name" "$value"
      fi
    done <"$RUST_SRC"

    echo "}"
  } >"$out"
}

for out in "${TF_OUT[@]}"; do
  generate_locals "$out"
done

terraform fmt "${TF_OUT[@]}" >/dev/null
printf 'Generated %s\n' "${TF_OUT[@]}"
