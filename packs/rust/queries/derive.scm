; Rust derivation queries (§8.6.3, D-072/D-073/D-078). The generic extractor
; reads the fixed capture vocabulary; the engine does resolution, attribution,
; the bare-occurrence validity filter (D-070g), and the crate module tree.

; ---- calls (§8.2) ----
(call_expression) @call
(call_expression function: (identifier) @call.callee) @call
(call_expression
  function: (field_expression
    value: (identifier) @call.receiver
    field: (field_identifier) @call.method)) @call

; ---- imports (§8.2, D-062c) ----
; `use a::b::name;` -- the module portion `a::b` (the outer path) resolves
; through the mod tree (rust_use_paths); `name` is the imported item. Grouped
; (`use a::{x, y}`), glob (`use a::*`), and aliased uses are not captured and
; drop (G-7).
(use_declaration
  argument: (scoped_identifier
    path: (_) @import.source
    name: (identifier) @import.name)) @import

; ---- module declarations (D-078) ----
; The name and the inline marker come from separate patterns; the engine keys
; both on the shared mod_item node.
(mod_item name: (identifier) @module.name) @module.decl
(mod_item body: (declaration_list)) @module.inline

; ---- state touches (§8.3, D-073) ----
(identifier) @touch.symbol
(field_expression
  value: (identifier) @touch.access.obj
  field: (field_identifier) @touch.access.attr) @touch.access
(assignment_expression left: (identifier) @touch.assign_lhs)
(assignment_expression left: (field_expression) @touch.assign_lhs)
(compound_assignment_expr left: (identifier) @touch.aug_assign_lhs)
(compound_assignment_expr left: (field_expression) @touch.aug_assign_lhs)
(call_expression
  function: (field_expression
    value: (identifier) @touch.receiver
    field: (field_identifier) @touch.call_function))
