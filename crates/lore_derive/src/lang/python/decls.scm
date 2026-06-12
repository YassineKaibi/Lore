; §8.1 derived nodes for Python (D-060a): functions/methods -> Function,
; classes -> Type. Assignments derive no node.
(function_definition name: (identifier) @name) @decl
(class_definition name: (identifier) @name) @decl
