package runtime

import "fmt"

// ErrorKind classifies why a call failed. The three wire-carried values
// mirror ferry-go's ErrorKind enum exactly (same string tags), so a
// structured error decoded from the envelope and one synthesized locally by
// the Go runtime (see Kind below) share one vocabulary.
type ErrorKind string

const (
	// KindPanic means the guest's closure panicked. On today's
	// wasm32-wasip1 toolchain this is never decoded from the envelope --
	// the prebuilt std forces panic=abort, so a panic traps the whole
	// module instance before the guest can write a response. Call
	// synthesizes KindPanic itself when the underlying export call
	// errors out, using captured stderr for the message. See docs/abi.md.
	KindPanic ErrorKind = "panic"
	// KindDecode means bytes that were expected to decode into a
	// particular shape didn't -- either the guest's input, or (on the
	// host side) the envelope/result coming back from the guest.
	KindDecode ErrorKind = "decode"
	// KindGuest means the guest's closure ran to completion and returned
	// an ordinary error.
	KindGuest ErrorKind = "guest"
)

// Error is what Call returns for any failed call, whether the failure was
// reported structurally by the guest or inferred by the host from a trap.
type Error struct {
	Kind    ErrorKind
	Message string
	// Stderr is the guest module's captured stderr for this call, if any.
	// It's most useful for KindPanic, where it may hold the panic message
	// printed by ferry-go's panic hook -- the only way that message
	// survives an abort.
	Stderr string
}

func (e *Error) Error() string {
	if e.Stderr != "" {
		return fmt.Sprintf("%s: %s (stderr: %s)", e.Kind, e.Message, e.Stderr)
	}
	return fmt.Sprintf("%s: %s", e.Kind, e.Message)
}
