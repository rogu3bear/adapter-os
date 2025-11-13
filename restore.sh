#!/bin/bash
brew services stop postgresql@15
dropdb -U adapteros adapteros_prod || true
createdb -U adapteros adapteros_prod
psql -U adapteros adapteros_prod < /backup/adapteros_latest.sql
brew services start postgresql@15
