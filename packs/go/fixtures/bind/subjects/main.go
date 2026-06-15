package app

// @lore
// purpose: "a function subject (@subject.function)"
func Alpha() {}

// @lore
// purpose: "a type subject (@subject.type)"
type Beta struct{}

// @lore
// kind: state
// name: Gamma
var Gamma int = 0

// @lore
// purpose: "a const subject (@subject.value)"
const Delta = 1

// @lore
// purpose: "a method subject (@subject.function)"
func (b Beta) Meth() {}
