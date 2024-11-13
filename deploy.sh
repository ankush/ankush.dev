#!/bin/bash


set -e

cd ~/projects/ankush_dev
git fetch origin develop
git reset --hard origin/develop
docker compose up -d --no-deps --build backend
docker image prune -f
docker compose restart
