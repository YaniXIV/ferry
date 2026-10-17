package runtime

// EnvelopeVersion must match ferry-go's ENVELOPE_VERSION. There is no
// code-level link between the two constants beyond this comment and
// testdata/envelope.msgpack, a fixture both test suites decode and assert
// the version field of -- see fixture_test.go and ferry-go's
// tests/fixture.rs. Bump both together if the envelope *shape* ever
// changes; this is independent of any single call's payload schema.
const EnvelopeVersion uint16 = 1

// wireEnvelope mirrors ferry_go::Envelope. Every exported guest function
// that returns normally (didn't trap) writes one of these.
type wireEnvelope struct {
	Version uint16 `msgpack:"v"`
	Ok      bool   `msgpack:"k"`
	Payload []byte `msgpack:"p"`
}

// wireError mirrors ferry_go::WireError, the envelope payload when Ok is
// false.
type wireError struct {
	Kind    ErrorKind `msgpack:"kind"`
	Message string    `msgpack:"message"`
}
