package main

import (
	"backlog-cli/internal/cli"
	"context"
	"os"
	"os/signal"
	"syscall"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	exit := cli.Execute(ctx, os.Args[1:], os.Stdout, os.Stderr, nil)
	stop()
	os.Exit(exit)
}
