#!/bin/bash
# Load .env file for development
if [ -f .env ]; then
    export $(grep -v "^#" .env | xargs)
    echo ".env variables loaded"
else
    echo ".env file not found"
fi
