; Rust declaration recognition (§7.4, D-050b, §8.6.3). The subject identifier
; is the `name` field for all declaration nodes. Functions -> Function;
; structs/enums/traits -> Type; statics/consts/mods and struct fields are
; value-binding (no derived node). Struct fields are bindable because
; idiomatic Rust state lives in them, so `kind: state` has a subject to attach
; to (D-084); a field's name is a `field_identifier`.
(function_item name: (identifier) @subject.name) @subject.function
(struct_item name: (type_identifier) @subject.name) @subject.type
(enum_item name: (type_identifier) @subject.name) @subject.type
(trait_item name: (type_identifier) @subject.name) @subject.type
(static_item name: (identifier) @subject.name) @subject.value
(const_item name: (identifier) @subject.name) @subject.value
(mod_item name: (identifier) @subject.name) @subject.value
(field_declaration name: (field_identifier) @subject.name) @subject.value
