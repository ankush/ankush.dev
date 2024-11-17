#!/bin/bash

set -e
set -x

cd ~/projects/ankush_dev
git fetch origin develop
git reset --hard origin/develop
docker compose pull
docker compose up -d --force-recreate --remove-orphans
docker image prune -f
