Place a custom PostgreSQL certificate authority PEM file here if needed.
Set `DB_SSL=true` and `DB_SSL_CA_FILE=/app/certs/postgres-ca.pem` in `.env`
when the file is named `postgres-ca.pem`. Compose mounts this directory
read-only; certificate files are not part of the image or release archive.
