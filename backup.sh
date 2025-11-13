#!/bin/bash
mkdir -p /backup
pg_dump -U adapteros adapteros_prod > /backup/adapteros_$(date +%Y%m%d).sql
tar czf /backup/configs_$(date +%Y%m%d).tar.gz /etc/adapteros/
