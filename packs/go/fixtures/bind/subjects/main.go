package app

// @veridikt
// purpose: "a function subject (@subject.function)"
func Alpha() {}

// @veridikt
// purpose: "a type subject (@subject.type)"
type Beta struct{}

// @veridikt
// kind: state
// name: Gamma
var Gamma int = 0

// @veridikt
// purpose: "a const subject (@subject.value)"
const Delta = 1

// @veridikt
// purpose: "a method subject (@subject.function)"
func (b Beta) Meth() {}
