#!/bin/bash
# Creates a self-signed code signing certificate called "Aede Local Signing" in your login
# keychain. Run it once: `bash tools/create-signing-cert.sh`.
#
# Why: macOS ties the microphone and accessibility permissions to the signature of the app.
# An ad hoc signature changes at every build, so the permissions are lost each time. A signature
# made with the same certificate stays the same, and tools/build-app.sh uses it when it exists.
#
# This is NOT an Apple certificate: it only works on this Mac, and it does not let anyone
# else open the app without a warning. Aède is distributed as source code only.
#
# The first build asks once whether codesign may use the key: choose "Always Allow".
set -euo pipefail

CERT_NAME="Aede Local Signing"
KEYCHAIN="${AEDE_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

if security find-identity -p codesigning "$KEYCHAIN" | grep -q "$CERT_NAME"; then
  echo "Le certificat \"$CERT_NAME\" existe déjà, rien à faire."
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PASSWORD="$(uuidgen)"

cat > "$WORK/openssl.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $CERT_NAME
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
EOF

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$WORK/openssl.cnf" \
  -keyout "$WORK/key.pem" -out "$WORK/cert.pem" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
  -out "$WORK/cert.p12" -passout "pass:$PASSWORD"

security import "$WORK/cert.p12" -k "$KEYCHAIN" -P "$PASSWORD" -T /usr/bin/codesign >/dev/null

echo "Certificat \"$CERT_NAME\" créé (valable 10 ans). Relance maintenant : npm run app"
