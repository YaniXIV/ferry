package greet_test

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/YaniXIV/ferry/examples/greet"
	"github.com/YaniXIV/ferry/runtime"
)

func newModule(t *testing.T) (context.Context, *runtime.Module) {
	t.Helper()
	ctx := context.Background()
	rt, err := runtime.NewRuntime(ctx)
	if err != nil {
		t.Fatalf("new runtime: %v", err)
	}
	t.Cleanup(func() { rt.Close(ctx) })
	return ctx, rt.NewModule("greet", greet.Load)
}

func TestGreetSuccess(t *testing.T) {
	ctx, mod := newModule(t)

	out, err := runtime.Call[greet.GreetIn, greet.GreetOut](ctx, mod, "greet", greet.GreetIn{Name: "ferry"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if out.Message != "hello, ferry" {
		t.Fatalf("got message %q", out.Message)
	}
}

func TestGreetGuestError(t *testing.T) {
	ctx, mod := newModule(t)

	_, err := runtime.Call[greet.FailIn, greet.FailOut](ctx, mod, "fail", greet.FailIn{Reason: "nope"})
	var ferr *runtime.Error
	if !errors.As(err, &ferr) {
		t.Fatalf("expected *runtime.Error, got %v", err)
	}
	if ferr.Kind != runtime.KindGuest {
		t.Fatalf("got kind %q, want %q", ferr.Kind, runtime.KindGuest)
	}
	if ferr.Message != "nope" {
		t.Fatalf("got message %q, want %q", ferr.Message, "nope")
	}
}

func TestGreetPanic(t *testing.T) {
	ctx, mod := newModule(t)

	_, err := runtime.Call[greet.PanicIn, greet.PanicOut](ctx, mod, "panic", greet.PanicIn{Message: "boom"})
	var ferr *runtime.Error
	if !errors.As(err, &ferr) {
		t.Fatalf("expected *runtime.Error, got %v", err)
	}
	if ferr.Kind != runtime.KindPanic {
		t.Fatalf("got kind %q, want %q", ferr.Kind, runtime.KindPanic)
	}
	// The guest trapped (wasm32-wasip1's prebuilt std forces panic=abort,
	// so ferry-go's catch_unwind never gets a chance to run -- see
	// docs/abi.md). The panic message only survives via the panic hook's
	// stderr print, which this asserts on instead of ferr.Message.
	if !strings.Contains(ferr.Stderr, "boom") {
		t.Fatalf("expected panic message in stderr, got %q", ferr.Stderr)
	}
}

func TestGreetDecodeError(t *testing.T) {
	ctx, mod := newModule(t)

	// greet's input type is { name: String }; sending an int where the
	// guest expects a string fails rmp_serde's deserialize inside
	// export(), producing a structured decode error from the guest.
	type badGreetIn struct {
		Name int `msgpack:"name"`
	}

	_, err := runtime.Call[badGreetIn, greet.GreetOut](ctx, mod, "greet", badGreetIn{Name: 42})
	var ferr *runtime.Error
	if !errors.As(err, &ferr) {
		t.Fatalf("expected *runtime.Error, got %v", err)
	}
	if ferr.Kind != runtime.KindDecode {
		t.Fatalf("got kind %q, want %q", ferr.Kind, runtime.KindDecode)
	}
}
