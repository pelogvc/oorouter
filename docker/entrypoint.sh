#!/bin/sh

set -eu

app_user=oorouter
app_group=oorouter
source_auth_path=/config/codex/auth.json
runtime_auth_dir=/run/oorouter/codex
runtime_auth_path="$runtime_auth_dir/auth.json"
app_data_root=/data
app_data_dir="$app_data_root/oorouter"

umask 077

if [ "$(id -u)" -ne 0 ]; then
    echo "oorouter container initialization requires root" >&2
    exit 1
fi

if [ ! -f "$source_auth_path" ]; then
    echo "Codex auth file is missing at /config/codex/auth.json" >&2
    exit 1
fi

install --directory --mode 0700 --owner "$app_user" --group "$app_group" "$runtime_auth_dir"
install --mode 0600 --owner "$app_user" --group "$app_group" \
    "$source_auth_path" "$runtime_auth_path"

chown "$app_user:$app_group" "$app_data_root"
chmod 0700 "$app_data_root"
install --directory --mode 0700 --owner "$app_user" --group "$app_group" "$app_data_dir"
chown -R "$app_user:$app_group" "$app_data_dir"

export AUTH_PATH="$runtime_auth_path"

exec /usr/bin/setpriv \
    --reuid="$app_user" \
    --regid="$app_group" \
    --init-groups \
    --no-new-privs \
    --inh-caps=-all \
    --ambient-caps=-all \
    /usr/bin/tini -- \
    /usr/local/bin/proxy-server \
    "$@" \
    --host 0.0.0.0
