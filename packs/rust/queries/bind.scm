; Rust declaration recognition (§7.4, D-050b, §8.6.3). The subject identifier
; is the `name` field for all seven declaration nodes. Functions -> Function;
; structs/enums/traits -> Type; statics/consts/mods are value-binding (no
; derived node).
(function_item name: (identifier) @subject.name) @subject.function
(struct_item name: (type_identifier) @subject.name) @subject.type
(enum_item name: (type_identifier) @subject.name) @subject.type
(trait_item name: (type_identifier) @subject.name) @subject.type
(static_item name: (identifier) @subject.name) @subject.value
(const_item name: (identifier) @subject.name) @subject.value
(mod_item name: (identifier) @subject.name) @subject.value
