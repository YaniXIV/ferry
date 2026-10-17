// Package greet embeds a tiny ferry-go guest exercising a successful call,
// a structured guest error, and a panic, as a worked example of how a
// Ferry consumer wires up a Module and calls it with runtime.Call.
//
//	rt, _ := runtime.NewRuntime(ctx)
//	defer rt.Close(ctx)
//	mod := rt.NewModule("greet", greet.Load)
//	out, err := runtime.Call[greet.GreetIn, greet.GreetOut](ctx, mod, "greet", greet.GreetIn{Name: "ferry"})
package greet

import (
	_ "embed"

	"github.com/YaniXIV/ferry/runtime"
)

//go:embed greet.wasm.zst
var wasmCompressed []byte

// Load decompresses the embedded guest module. It's a runtime.Loader,
// meant to be passed directly to Runtime.NewModule.
func Load() ([]byte, error) {
	return runtime.LoadZstd(wasmCompressed)
}

// GreetIn / GreetOut mirror the guest's greet export: always succeeds.
type GreetIn struct {
	Name string `msgpack:"name"`
}

type GreetOut struct {
	Message string `msgpack:"message"`
}

// FailIn mirrors the guest's fail export, which always returns a guest
// error carrying Reason as the message.
type FailIn struct {
	Reason string `msgpack:"reason"`
}

type FailOut struct{}

// PanicIn mirrors the guest's panic export, which always panics with
// Message.
type PanicIn struct {
	Message string `msgpack:"message"`
}

type PanicOut struct{}
