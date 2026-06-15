; TypeScript derivation queries (§8.6.3, D-072/D-073). The generic extractor
; reads the fixed capture vocabulary; the engine does resolution, attribution,
; and the bare-occurrence validity filter (D-070g).

; ---- calls (§8.2) ----
; `new Cls()` is a new_expression, not a call_expression, so a construction is
; not counted as a call (matches the pre-T8 TS behavior, unlike Python).
(call_expression) @call
(call_expression function: (identifier) @call.callee) @call
(call_expression
  function: (member_expression
    object: (identifier) @call.receiver
    property: (property_identifier) @call.method)) @call
; Local construction `const x = new Cls()` for the D-062e method-call rule.
(variable_declarator
  name: (identifier) @call.construct.var
  value: (new_expression constructor: (identifier) @call.construct.class)) @call.construct

; ---- imports (§8.2, D-062c) ----
; `import { n } from "m"` / `import { n as a } from "m"` — the `!alias`
; negation keeps the plain and aliased patterns mutually exclusive. Default
; imports are not captured and drop (G-7).
(import_statement
  (import_clause (named_imports (import_specifier name: (identifier) @import.name !alias)))
  source: (string (string_fragment) @import.source)) @import
(import_statement
  (import_clause (named_imports (import_specifier
    name: (identifier) @import.name
    alias: (identifier) @import.alias)))
  source: (string (string_fragment) @import.source)) @import
; `import * as a from "m"`
(import_statement
  (import_clause (namespace_import (identifier) @import.namespace))
  source: (string (string_fragment) @import.source)) @import

; ---- state touches (§8.3, D-073) ----
(identifier) @touch.symbol
(member_expression
  object: (identifier) @touch.access.obj
  property: (property_identifier) @touch.access.attr) @touch.access
(assignment_expression left: (identifier) @touch.assign_lhs)
(assignment_expression left: (member_expression) @touch.assign_lhs)
(augmented_assignment_expression left: (identifier) @touch.aug_assign_lhs)
(augmented_assignment_expression left: (member_expression) @touch.aug_assign_lhs)
(call_expression
  function: (member_expression
    object: (identifier) @touch.receiver
    property: (property_identifier) @touch.call_function))
(call_expression
  function: (member_expression
    object: (member_expression) @touch.receiver
    property: (property_identifier) @touch.call_function))
