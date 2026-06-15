package app

// @lore
// purpose: "this block binds to an import, not a §7.4 declaration — E0102"
import "fmt"

func use() { fmt.Println("x") }
