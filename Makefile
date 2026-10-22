TOOLS_BIN := $(CURDIR)/.tools/bin
GOLANGCI_LINT := $(TOOLS_BIN)/golangci-lint
GOVULNCHECK := $(TOOLS_BIN)/govulncheck

.PHONY: tools fmt check test-race security build

tools:
	GOBIN=$(TOOLS_BIN) go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@v2.9.0
	GOBIN=$(TOOLS_BIN) go install golang.org/x/vuln/cmd/govulncheck@v1.9.0

fmt:
	$(GOLANGCI_LINT) fmt ./...

check:
	$(GOLANGCI_LINT) fmt --diff ./...
	$(GOLANGCI_LINT) run ./...
	go test ./...
	go vet ./...

test-race:
	go test -race ./...

security:
	$(GOVULNCHECK) -test ./...

build:
	CGO_ENABLED=0 GOOS=darwin GOARCH=arm64 go build -trimpath -o dist/bl ./cmd/bl
