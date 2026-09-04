#!/usr/bin/env bash
# Create a self-signed code-signing identity in the login keychain so rdc.app has a stable
# designated requirement (TCC grants survive rebuilds). Idempotent.
# Usage: scripts/macos/make-signing-identity.sh [name]   (default: rdc-dev)
set -euo pipefail
NAME="${1:-rdc-dev}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
if security find-identity -v -p codesigning "$KEYCHAIN" | grep -q "\"$NAME\""; then
  echo "identity '$NAME' already present"; exit 0
fi
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
security import "$TMP/id.p12" -k "$KEYCHAIN" -P rdc -T /usr/bin/codesign -T /usr/bin/security >/dev/null
# Trust it for code signing in the user's trust domain (no admin rights needed).
if ! security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem" 2>"$TMP/trust.err"; then
  echo "could not set trust automatically: $(cat "$TMP/trust.err")" >&2
  echo "Open Keychain Access on the Mac, find certificate '$NAME' (login keychain), Get Info → Trust → Code Signing: Always Trust." >&2
fi
# Let codesign use the private key without a GUI prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "" "$KEYCHAIN" >/dev/null 2>&1 || true
security find-identity -v -p codesigning "$KEYCHAIN" | grep "$NAME" || { echo "identity '$NAME' is not (yet) valid for code signing" >&2; exit 1; }
