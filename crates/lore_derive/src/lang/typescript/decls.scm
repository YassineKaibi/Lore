; §8.1 derived nodes for TypeScript (D-060a): functions/methods -> Function,
; classes/interfaces/type aliases/enums -> Type. Variable declarations
; derive no node.
(function_declaration name: (identifier) @name) @decl
(class_declaration name: (type_identifier) @name) @decl
(method_definition name: (property_identifier) @name) @decl
(interface_declaration name: (type_identifier) @name) @decl
(type_alias_declaration name: (type_identifier) @name) @decl
(enum_declaration name: (identifier) @name) @decl
