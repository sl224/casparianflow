#!/bin/bash
# Docker test infrastructure helper script
#
# Usage:
#   ./scripts/docker-test.sh up      # Start all test containers
#   ./scripts/docker-test.sh down    # Stop all test containers
#   ./scripts/docker-test.sh status  # Show container status
#   ./scripts/docker-test.sh test    # Run docker tests
#   ./scripts/docker-test.sh pg      # Start only PostgreSQL containers
#   ./scripts/docker-test.sh mssql   # Start only MSSQL containers

set -e

COMPOSE_FILE="crates/casparian_test_utils/docker/docker-compose.yml"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# Use docker compose v2 if available, fallback to docker-compose v1
compose() {
    if docker compose version &>/dev/null; then
        docker compose -f "$COMPOSE_FILE" "$@"
    else
        docker-compose -f "$COMPOSE_FILE" "$@"
    fi
}

case "${1:-help}" in
    up)
        echo "Starting all test database containers..."
        compose up -d
        echo ""
        echo "Waiting for containers to be healthy..."
        sleep 10
        compose ps
        echo ""
        echo "Containers started. Run tests with:"
        echo "  cargo test -p casparian_test_utils --features docker-tests"
        ;;

    down)
        echo "Stopping all test containers and removing volumes..."
        compose down -v
        echo "Done."
        ;;

    status)
        echo "Container status:"
        compose ps
        ;;

    pg|postgres)
        echo "Starting PostgreSQL containers only..."
        compose up -d postgres14 postgres15 postgres16
        echo "Waiting for containers to be healthy..."
        sleep 5
        compose ps postgres14 postgres15 postgres16
        ;;

    mssql)
        echo "Starting MSSQL containers only..."
        compose up -d mssql2019 mssql2022
        echo "Waiting for containers to be healthy (MSSQL takes longer)..."
        sleep 30
        compose ps mssql2019 mssql2022
        ;;

    test)
        echo "Running docker tests..."
        cargo test -p casparian_test_utils --features docker-tests -- --test-threads=1
        ;;

    test-pg)
        echo "Running PostgreSQL tests only..."
        cargo test -p casparian_test_utils --features docker-tests postgres -- --test-threads=1
        ;;

    test-mssql)
        echo "Running MSSQL tests only..."
        cargo test -p casparian_test_utils --features docker-tests,mssql mssql -- --test-threads=1
        ;;

    logs)
        service="${2:-}"
        if [ -n "$service" ]; then
            compose logs -f "$service"
        else
            compose logs -f
        fi
        ;;

    help|*)
        echo "Docker test infrastructure helper"
        echo ""
        echo "Usage: $0 <command>"
        echo ""
        echo "Commands:"
        echo "  up        Start all test database containers"
        echo "  down      Stop all containers and remove volumes"
        echo "  status    Show container status"
        echo "  pg        Start PostgreSQL containers only"
        echo "  mssql     Start MSSQL containers only"
        echo "  test      Run all docker tests"
        echo "  test-pg   Run PostgreSQL tests only"
        echo "  test-mssql Run MSSQL tests only"
        echo "  logs [svc] Show container logs (optionally for specific service)"
        echo "  help      Show this help message"
        echo ""
        echo "Port mapping:"
        echo "  PostgreSQL 14: localhost:15432"
        echo "  PostgreSQL 15: localhost:15433"
        echo "  PostgreSQL 16: localhost:15434"
        echo "  MSSQL 2019:    localhost:11433"
        echo "  MSSQL 2022:    localhost:11434"
        ;;
esac
