; Go derivation queries (§8.6.3, D-072/D-073). The generic extractor reads the
; fixed capture vocabulary; the engine does resolution, attribution, and the
; bare-occurrence validity filter (D-070g).

; ---- calls (§8.2) ----
(call_expression) @call
(call_expression function: (identifier) @call.callee) @call
(call_expression
  function: (selector_expression
    operand: (identifier) @call.receiver
    field: (field_identifier) @call.method)) @call

; ---- imports (§8.2, D-062c/D-076) ----
; `import "a/b/helpers"` -- whole-package; whole_alias = last_segment makes the
; binding name the path tail `helpers`. `import h "a/b"` gives the explicit
; alias `h`.
(import_spec
  path: (interpreted_string_literal (interpreted_string_literal_content) @import.source)) @import
(import_spec
  name: (package_identifier) @import.namespace
  path: (interpreted_string_literal (interpreted_string_literal_content) @import.source)) @import

; ---- state touches (§8.3, D-073) ----
(identifier) @touch.symbol
(selector_expression
  operand: (identifier) @touch.access.obj
  field: (field_identifier) @touch.access.attr) @touch.access
(assignment_statement left: (expression_list (identifier) @touch.assign_lhs))
(assignment_statement left: (expression_list (selector_expression) @touch.assign_lhs))
(inc_statement (identifier) @touch.aug_assign_lhs)
(dec_statement (identifier) @touch.aug_assign_lhs)
(call_expression
  function: (selector_expression
    operand: (identifier) @touch.receiver
    field: (field_identifier) @touch.call_function))
; free-function mutator `delete(sym, ...)` -- the first argument is mutated.
(call_expression
  function: (identifier) @touch.call_function
  arguments: (argument_list . (identifier) @touch.receiver))
