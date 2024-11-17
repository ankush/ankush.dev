#!/bin/bash

set -e
set -x

cd ~/projects/ankush_dev
git fetch origin develop
git reset --hard origin/develop
docker compose pull
docker compose stop
docker compose rm -f backend
docker compose up -d
docker system prune -f
