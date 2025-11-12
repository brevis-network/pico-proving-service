.PHONY: help up down logs restart status exec clean download-gnark
.PHONY: logs-server logs-gnark

# Docker Compose settings
COMPOSE := $(shell command -v docker-compose 2>/dev/null || echo "docker compose")
DATA_DIR := $(PWD)/../data

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Service Management:'
	@echo '  up              - Start all services (server + gnark)'
	@echo '  down            - Stop all services'
	@echo '  restart         - Restart all services'
	@echo '  status          - Show service status'
	@echo ''
	@echo 'Logs:'
	@echo '  logs            - View logs from all services'
	@echo '  logs-server     - View logs from server only'
	@echo '  logs-gnark      - View logs from gnark only'
	@echo ''
	@echo 'Utilities:'
	@echo '  download-gnark  - Download gnark verification files'
	@echo '  exec            - Access server container shell'
	@echo '  clean           - Stop and remove all containers'
	@echo ''

download-gnark: ## Download gnark verification files
	./download-gnark.sh

up: ## Start all services
	@if [ ! -f .env ]; then \
		echo "Error: .env file not found. Please copy .env.example to .env and configure it."; \
		exit 1; \
	fi
	@if [ ! -d "../gnark_downloads/kb" ] || [ ! -f "../gnark_downloads/kb/vm_pk" ]; then \
		echo "Warning: gnark verification files not found. On-chain proving will not work."; \
		echo "Run 'make download-gnark' to download them."; \
	fi
	@echo "Creating data directory: $(DATA_DIR)"
	@mkdir -p $(DATA_DIR)
	@if [ ! -f "$(DATA_DIR)/pico_proving_service.db" ]; then \
		echo "Database file not found. Initializing database..."; \
		if ! command -v sqlx >/dev/null 2>&1; then \
			echo "Error: sqlx-cli not found. Please install it:"; \
			echo "  cargo install sqlx-cli"; \
			exit 1; \
		fi; \
		cd ../.. && sqlx database create --database-url sqlite://docker/data/pico_proving_service.db?mode=rwc && \
		sqlx migrate run --source migrations --database-url sqlite://docker/data/pico_proving_service.db?mode=rwc && \
		echo "Database initialized successfully."; \
	fi
	@if ! docker images --format "{{.Repository}}:{{.Tag}}" | grep -q "^pico-proving-service-cpu:latest$$"; then \
		echo "Error: Docker image 'pico-proving-service-cpu:latest' not found"; \
		echo "Please load the Docker image first:"; \
		echo "  docker load -i pico-proving-service-cpu.tar"; \
		exit 1; \
	fi
	$(COMPOSE) up -d

down: ## Stop all services
	$(COMPOSE) down

logs: ## View logs from all services
	$(COMPOSE) logs -f

logs-server: ## View logs from server only
	$(COMPOSE) logs -f server

logs-gnark: ## View logs from gnark only
	$(COMPOSE) logs -f gnark

restart: ## Restart all services
	$(COMPOSE) restart

status: ## Show service status
	@echo "Service status:"
	$(COMPOSE) ps
	@echo ""
	@echo "Data directory: $(DATA_DIR)"
	@if [ -d "$(DATA_DIR)" ]; then \
		echo "Database files:"; \
		ls -lh $(DATA_DIR)/ 2>/dev/null || echo "  (empty)"; \
	else \
		echo "  (not created yet)"; \
	fi

exec: ## Execute bash in the server container
	docker exec -it pico-proving-service-cpu bash

clean: ## Stop and remove all containers
	$(COMPOSE) down
	@echo "Cleanup complete. Data directory preserved at: $(DATA_DIR)"
	@echo "To remove data: rm -rf $(DATA_DIR)"
