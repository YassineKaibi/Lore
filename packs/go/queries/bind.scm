; Go declaration recognition (§7.4, §8.6.3). Functions/methods -> Function;
; type declarations -> Type; var/const are value-binding (D-060a). A var/const
; spec with two names matches @subject.name twice, so the engine demands an
; explicit name: (E0104).
(function_declaration name: (identifier) @subject.name) @subject.function
(method_declaration name: (field_identifier) @subject.name) @subject.function
(type_declaration (type_spec name: (type_identifier) @subject.name)) @subject.type
(var_declaration (var_spec name: (identifier) @subject.name)) @subject.value
(const_declaration (const_spec name: (identifier) @subject.name)) @subject.value
