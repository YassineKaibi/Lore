; Java declaration recognition (§7.4, §8.6.3). Methods/constructors ->
; Function; classes/interfaces/enums/records -> Type; fields are value-binding
; (D-060a). A field with two declarators matches @subject.name twice, so the
; engine demands an explicit name: (E0104).
(method_declaration name: (identifier) @subject.name) @subject.function
(constructor_declaration name: (identifier) @subject.name) @subject.function
(class_declaration name: (identifier) @subject.name) @subject.type
(interface_declaration name: (identifier) @subject.name) @subject.type
(enum_declaration name: (identifier) @subject.name) @subject.type
(record_declaration name: (identifier) @subject.name) @subject.type
(field_declaration declarator: (variable_declarator name: (identifier) @subject.name)) @subject.value
