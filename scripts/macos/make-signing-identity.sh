#!/usr/bin/env bash
# Create a self-signed code-signing identity for rdc in a DEDICATED keychain whose password is
# stored in a 0600 file, so scripts can sign over SSH (the login keychain is locked there).
# The identity only exists to give rdc.app a stable designated requirement so macOS TCC grants
# (Screen Recording, Accessibility) survive rebuilds.
#
# Run this ONCE from Terminal on the Mac (a GUI session; the trust step may ask for your login
# password). Idempotent. Usage: make-signing-identity.sh [name]   (default: rdc-dev)
set -euo pipefail
NAME="${1:-rdc-dev}"
KC="$HOME/Library/Keychains/rdc-signing.keychain-db"
PASSFILE="$HOME/.config/rdc/signing-keychain-pass"
LOGIN="$HOME/Library/Keychains/login.keychain-db"

mkdir -p "$(dirname "$PASSFILE")"
if [ ! -s "$PASSFILE" ]; then
  (umask 077; head -c 24 /dev/urandom | xxd -p | tr -d '\n' > "$PASSFILE")
fi
PASS=$(cat "$PASSFILE")

if [ ! -f "$KC" ]; then
  security create-keychain -p "$PASS" "$KC"
  security set-keychain-settings "$KC"            # no auto-lock, no timeout
fi
security unlock-keychain -p "$PASS" "$KC"
# Add to the user's search list (keep existing entries).
CURRENT=$(security list-keychains -d user | tr -d '" ' )
if ! grep -q "rdc-signing.keychain-db" <<<"$CURRENT"; then
  # shellcheck disable=SC2086
  security list-keychains -d user -s $CURRENT "$KC"
fi

if security find-identity -v -p codesigning "$KC" | grep -q "\"$NAME\""; then
  echo "identity '$NAME' already present in $KC"
else
  TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
  cat > "$TMP/ext.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = $NAME
[v3]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
subjectKeyIdentifier = hash
CNF
  openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/ext.cnf" >/dev/null 2>&1
  openssl pkcs12 -export -legacy -out "$TMP/id.p12" -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -passout pass:rdc 2>/dev/null \
    || openssl pkcs12 -export -out "$TMP/id.p12" -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -passout pass:rdc
  security import "$TMP/id.p12" -k "$KC" -P rdc -T /usr/bin/codesign -T /usr/bin/security >/dev/null
  # Trust for code signing (user trust domain). This is the step that may prompt for your password.
  security add-trusted-cert -r trustRoot -p codeSign -k "$KC" "$TMP/cert.pem" \
    || echo "trust step failed; in Keychain Access set '$NAME' → Trust → Code Signing: Always Trust" >&2
fi
# Let codesign use the key without a GUI prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PASS" "$KC" >/dev/null

# Remove an older copy of the identity from the login keychain so codesign isn't ambiguous.
if security find-identity -v -p codesigning "$LOGIN" 2>/dev/null | grep -q "\"$NAME\""; then
  echo "removing older '$NAME' identity from the login keychain"
  security delete-identity -c "$NAME" -t "$LOGIN" >/dev/null 2>&1 || true
fi

security find-identity -v -p codesigning "$KC" | grep "$NAME" || { echo "identity '$NAME' is not valid for code signing" >&2; exit 1; }
echo "ok: $KC (password in $PASSFILE)"
