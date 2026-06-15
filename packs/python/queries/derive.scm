; Python derivation queries (§8.6.3, D-072/D-073). The generic extractor
; reads the fixed capture vocabulary; the engine does resolution, attribution,
; and the bare-occurrence validity filter (D-070g).

; ---- calls (§8.2) ----
; Every call counts (opaque ones drop and are counted, §8.2 rule 3); the
; decomposition patterns add the callee parts (D-072).
(call) @call
(call function: (identifier) @call.callee) @call
(call
  function: (attribute
    object: (identifier) @call.receiver
    attribute: (identifier) @call.method)) @call
; Local construction `x = Cls()` for the D-062e method-call rule.
(assignment
  left: (identifier) @call.construct.var
  right: (call function: (identifier) @call.construct.class)) @call.construct

; ---- imports (§8.2, D-062c) ----
; `from m import n` / `from m import n as a` (relative `from . import` has no
; dotted module_name, so it is not captured and drops — G-7).
(import_from_statement
  module_name: (dotted_name) @import.source
  name: (dotted_name) @import.name) @import
(import_from_statement
  module_name: (dotted_name) @import.source
  name: (aliased_import
    name: (dotted_name) @import.name
    alias: (identifier) @import.alias)) @import
; `import m` / `import a.b` (whole-module; the engine defaults the namespace
; binding to the source when no alias is present).
(import_statement name: (dotted_name) @import.source) @import
; `import x as a`
(import_statement
  name: (aliased_import
    name: (dotted_name) @import.source
    alias: (identifier) @import.namespace)) @import

; ---- state touches (§8.3, D-073) ----
(identifier) @touch.symbol
(attribute
  object: (identifier) @touch.access.obj
  attribute: (identifier) @touch.access.attr) @touch.access
(assignment left: (identifier) @touch.assign_lhs)
(assignment left: (attribute) @touch.assign_lhs)
(augmented_assignment left: (identifier) @touch.aug_assign_lhs)
(augmented_assignment left: (attribute) @touch.aug_assign_lhs)
(call
  function: (attribute
    object: (identifier) @touch.receiver
    attribute: (identifier) @touch.call_function))
(call
  function: (attribute
    object: (attribute) @touch.receiver
    attribute: (identifier) @touch.call_function))
