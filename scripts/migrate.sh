#!/bin/bash
cd /Users/star/Dev/adapter-os
target/release/adapteros-server --config /etc/adapteros/cp.toml --migrate-only
