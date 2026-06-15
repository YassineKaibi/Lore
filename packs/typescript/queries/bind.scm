; TypeScript declaration recognition (§7.4, §8.6.3). Functions/methods ->
; Function; classes/interfaces/type aliases/enums -> Type; variable
; declarations are value-binding (D-060a). A lexical/variable declaration with
; two declarators matches @subject.name twice, so the engine leaves the name
; unresolved and demands an explicit name: (E0104).
(function_declaration name: (identifier) @subject.name) @subject.function
(method_definition name: (property_identifier) @subject.name) @subject.function
(class_declaration name: (type_identifier) @subject.name) @subject.type
(interface_declaration name: (type_identifier) @subject.name) @subject.type
(type_alias_declaration name: (type_identifier) @subject.name) @subject.type
(enum_declaration name: (identifier) @subject.name) @subject.type
(lexical_declaration (variable_declarator name: (identifier) @subject.name)) @subject.value
(variable_declaration (variable_declarator name: (identifier) @subject.name)) @subject.value
