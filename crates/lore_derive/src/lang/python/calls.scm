; Every call expression (§8.2: classified in code, dropped calls counted),
; plus the local-construction pattern the method-call rule needs (D-062e).
(call) @call
(assignment
  left: (identifier) @var
  right: (call function: (identifier) @cls)) @construct
