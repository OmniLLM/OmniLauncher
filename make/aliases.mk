# -- Compatibility aliases -----------------------------------------------------

build-frontend:
	$(MAKE) build ROLE=frontend

build-backend:
	$(MAKE) build ROLE=backend

stop-frontend:
	$(MAKE) stop ROLE=frontend

stop-backend:
	$(MAKE) stop ROLE=backend

stop-all:
	$(MAKE) stop ROLE=both

start-frontend:
	$(MAKE) start ROLE=frontend

start-backend:
	$(MAKE) start ROLE=backend

start-wsl-backend:
	$(MAKE) start ROLE=backend BACKEND_MODE=wsl

restart-frontend:
	$(MAKE) restart ROLE=frontend

restart-backend:
	$(MAKE) restart ROLE=backend

restart-wsl-backend:
	$(MAKE) restart ROLE=backend BACKEND_MODE=wsl

prod-debug-backend:
	$(MAKE) stop ROLE=backend
	$(MAKE) start ROLE=backend DEBUG=1

prod-debug-frontend:
	$(MAKE) stop ROLE=frontend
	$(MAKE) start ROLE=frontend DEBUG=1

prod-debug:
	$(MAKE) stop ROLE=both
	$(MAKE) start ROLE=both DEBUG=1

clean-frontend:
	$(MAKE) clean ROLE=frontend

clean-backend:
	$(MAKE) clean ROLE=backend

test-frontend:
	$(MAKE) test KIND=frontend

test-rust:
	$(MAKE) test KIND=rust

test-unit:
	$(MAKE) test KIND=unit

test-backend:
	$(MAKE) test KIND=backend

test-health:
	$(MAKE) test KIND=health

test-smoke:
	$(MAKE) test KIND=smoke

test-e2e:
	$(MAKE) test KIND=e2e

test-all:
	$(MAKE) test KIND=all
