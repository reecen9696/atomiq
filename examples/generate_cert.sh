#!/bin/bash
# Generate self-signed certificate for HTTPS testing

echo "🔐 Generating self-signed certificate for HTTPS testing..."

# Create certificates directory
mkdir -p certs

# Generate private key
openssl genrsa -out certs/server.key 2048

# Generate certificate signing request
openssl req -new -key certs/server.key -out certs/server.csr \
  -subj "/C=US/ST=CA/L=San Francisco/O=Atomiq/CN=localhost"

# Generate self-signed certificate
openssl x509 -req -in certs/server.csr -signkey certs/server.key \
  -out certs/server.crt -days 365

# Convert to PEM format (for rustls)
cat certs/server.crt > certs/server.pem
cat certs/server.key >> certs/server.pem

echo "✅ Certificates generated:"
echo "   Certificate: certs/server.crt"
echo "   Private Key: certs/server.key"
echo "   PEM Bundle: certs/server.pem"
echo ""
echo "🚀 To run with HTTPS:"
echo "   cargo run --bin atomiq-api -- --tls --cert-path certs/server.crt --key-path certs/server.key"
echo ""
echo "📝 For production, use Let's Encrypt or a proper CA certificate"

# Clean up CSR
rm certs/server.csr