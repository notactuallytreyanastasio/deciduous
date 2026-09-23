#!/bin/sh
# Throwaway CA and database certificate for container TLS acceptance tests.
set -eu
umask 077
openssl req -x509 -newkey rsa:2048 -nodes -days 2 \
  -subj /CN=Deciduous-Release-Test-CA \
  -keyout /certs/ca.key -out /certs/ca.crt >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -subj /CN=db \
  -addext subjectAltName=DNS:db \
  -keyout /certs/server.key -out /certs/server.csr >/dev/null 2>&1
openssl x509 -req -days 2 -in /certs/server.csr \
  -CA /certs/ca.crt -CAkey /certs/ca.key -CAcreateserial \
  -copy_extensions copy -out /certs/server.crt >/dev/null 2>&1
chmod 644 /certs/ca.crt /certs/server.crt
