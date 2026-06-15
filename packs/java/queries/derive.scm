; Java derivation queries (§8.6.3, D-072/D-073). The generic extractor reads
; the fixed capture vocabulary; the engine does resolution, attribution, and
; the bare-occurrence validity filter (D-070g).

; ---- calls (§8.2) ----
(method_invocation) @call
(method_invocation !object name: (identifier) @call.callee) @call
(method_invocation
  object: (identifier) @call.receiver
  name: (identifier) @call.method) @call

; ---- imports (§8.2, D-062c/D-076) ----
; `import a.b.C;` -- a single-type import; whole_alias = last_segment makes the
; binding name the path tail `C`. Wildcard (`import a.*;`) and static imports
; are not captured and drop (G-7).
(import_declaration (scoped_identifier) @import.source) @import

; ---- state touches (§8.3, D-073) ----
(identifier) @touch.symbol
(field_access
  object: (identifier) @touch.access.obj
  field: (identifier) @touch.access.attr) @touch.access
(assignment_expression left: (identifier) @touch.assign_lhs)
(assignment_expression left: (field_access) @touch.assign_lhs)
(method_invocation
  object: (identifier) @touch.receiver
  name: (identifier) @touch.call_function)
