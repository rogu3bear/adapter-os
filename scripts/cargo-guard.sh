#!/usr/bin/env bash
# Guardrail script to check for active cargo processes

# Get all cargo processes, ignoring the grep itself and this script
CARGO_PROCS=$(ps -eo pid,etime,command | awk '/[c]argo/ && !/cargo-guard.sh/ {print $0}')

if [ -n "$CARGO_PROCS" ]; then
    echo "ERROR: Active cargo processes are currently running!"
    echo "Wait for them to finish before initiating a new cargo command."
    echo "If you are an AI agent, DO NOT attempt to bypass this lock and DO NOT run your cargo command."
    echo "Report the following running processes to the user and ask for instructions:"
    echo "---"
    echo "$CARGO_PROCS"
    echo "---"
    exit 1
fi

echo "No active cargo processes found. Safe to proceed."
exit 0
