package app

import "example.com/app/helpers"
import "example.com/app/missing"

// @lore
// kind: state
// name: counter
var counter int = 0

// @lore
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

// @lore
// purpose: "the same-file Exact-call target"
func local() {}

// @lore
// purpose: "reads the state symbol — a non-write occurrence"
func Show() int {
	return counter
}
