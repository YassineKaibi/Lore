; Every call expression (§8.2: classified in code, dropped calls counted),
; plus the local-construction pattern the method-call rule needs (D-062e).
(call_expression) @call
(variable_declarator
  name: (identifier) @var
  value: (new_expression constructor: (identifier) @cls)) @construct
