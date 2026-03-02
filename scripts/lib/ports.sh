#!/usr/bin/env bash
# shellcheck shell=bash
# Canonical local port pane contract for adapterOS.
#
# Defaults are derived from AOS_PORT_PANE_BASE (default 18080) unless a
# specific port/url env var is already set.

_aos_is_uint() {
  [[ "${1:-}" =~ ^[0-9]+$ ]]
}

aos_port_pane_base() {
  local raw="${AOS_PORT_PANE_BASE:-18080}"
  if ! _aos_is_uint "$raw" || (( raw < 1 || raw > 65523 )); then
    raw=18080
  fi
  printf "%s" "$raw"
}

aos_port_from_offset() {
  local offset="${1:-0}"
  local base
  base="$(aos_port_pane_base)"
  printf "%s" "$((base + offset))"
}

aos_set_port_defaults() {
  : "${AOS_PORT_PANE_BASE:=$(aos_port_pane_base)}"

  : "${AOS_SERVER_PORT:=$(aos_port_from_offset 0)}"
  : "${AOS_UI_PORT:=$(aos_port_from_offset 1)}"
  : "${AOS_PANEL_PORT:=$(aos_port_from_offset 2)}"
  : "${AOS_NODE_PORT:=$(aos_port_from_offset 3)}"
  : "${AOS_PROMETHEUS_PORT:=$(aos_port_from_offset 4)}"
  : "${AOS_MODEL_SERVER_PORT:=$(aos_port_from_offset 5)}"
  : "${AOS_CODEGRAPH_PORT:=$(aos_port_from_offset 6)}"
  : "${AOS_MINIMAL_UI_PORT:=$(aos_port_from_offset 7)}"
  : "${AOS_OTLP_PORT:=$(aos_port_from_offset 8)}"
  : "${AOS_VAULT_PORT:=$(aos_port_from_offset 9)}"
  : "${AOS_KMS_EMULATOR_PORT:=$(aos_port_from_offset 10)}"
  : "${AOS_POSTGRES_PORT:=$(aos_port_from_offset 11)}"
  : "${AOS_LOCALSTACK_PORT:=$(aos_port_from_offset 12)}"
}

aos_set_url_defaults() {
  : "${AOS_SERVER_HOST:=127.0.0.1}"
  : "${AOS_SERVER_URL:=http://${AOS_SERVER_HOST}:${AOS_SERVER_PORT}}"
  : "${AOS_API_URL:=http://${AOS_SERVER_HOST}:${AOS_SERVER_PORT}/api}"
  : "${AOS_UI_URL:=http://${AOS_SERVER_HOST}:${AOS_UI_PORT}}"
  : "${AOS_CP_URL:=http://${AOS_SERVER_HOST}:${AOS_SERVER_PORT}}"
  : "${AOS_MODEL_SERVER_ADDR:=http://${AOS_SERVER_HOST}:${AOS_MODEL_SERVER_PORT}}"
  : "${OTEL_EXPORTER_OTLP_ENDPOINT:=http://localhost:${AOS_OTLP_PORT}}"
  : "${PROMETHEUS_URL:=http://localhost:${AOS_PROMETHEUS_PORT}}"
  : "${AOS_KMS_EMULATOR_HOST:=127.0.0.1:${AOS_KMS_EMULATOR_PORT}}"
}

aos_apply_port_pane_defaults() {
  aos_set_port_defaults
  aos_set_url_defaults

  export AOS_PORT_PANE_BASE
  export AOS_SERVER_PORT AOS_UI_PORT AOS_PANEL_PORT AOS_NODE_PORT
  export AOS_PROMETHEUS_PORT AOS_MODEL_SERVER_PORT AOS_CODEGRAPH_PORT
  export AOS_MINIMAL_UI_PORT AOS_OTLP_PORT AOS_VAULT_PORT
  export AOS_KMS_EMULATOR_PORT AOS_POSTGRES_PORT AOS_LOCALSTACK_PORT
  export AOS_SERVER_HOST AOS_SERVER_URL AOS_API_URL AOS_UI_URL AOS_CP_URL
  export AOS_MODEL_SERVER_ADDR OTEL_EXPORTER_OTLP_ENDPOINT PROMETHEUS_URL
  export AOS_KMS_EMULATOR_HOST
}
