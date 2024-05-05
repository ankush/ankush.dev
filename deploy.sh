#!/bin/bash


set -e

cd ~/projects/ankush_dev
git pull
docker compose up -d --no-deps --build backend
docker image prune -f
