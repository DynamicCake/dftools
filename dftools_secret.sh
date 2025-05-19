#!/usr/bin/env bash
arg="$1"

case "$arg" in
    "gen-key")
        openssl rand 32 | base64 -w 0 | tr '+/' '-_'
        ;;
    "gen-jwt")
        openssl rand 64 | base64 -w 0 | tr '+/' '-_'
        ;;
    "")
        echo "Usage: dftools_secret.sh (gen-key|gen-jwt)"
        ;;
    *)
        echo "Unknown operation $arg"
        ;;
esac
