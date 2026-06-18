package app

import "example.com/app/helpers"
import "example.com/app/missing"

// @veridikt
// kind: state
// name: counter
var counter int = 0

// @veridikt
// purpose: "same-file, same-package, and cross-package calls, dropped calls, a write"
func Driver() {
	local()
	Greet()
	helpers.Hello()
	missing.Gone()
	Ghost()
	counter = counter + 1
	counter++
}

// @veridikt
// purpose: "the same-file Exact-call target"
func local() {}

// @veridikt
// purpose: "reads the state symbol — a non-write occurrence"
func Show() int {
	return counter
}
