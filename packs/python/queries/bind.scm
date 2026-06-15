; Python declaration recognition (§7.4, §8.6.3). Each pattern marks a §7.4
; declaration node with @subject.function|type|value and captures its
; identifier as @subject.name. Functions -> Function, classes -> Type,
; assignments are value-binding (bindable, no derived node — D-060a).
;
; The bare `(assignment) @subject.value` catch-all makes multi-target forms
; (`a, b = ...`) bindable with no @subject.name, so the engine demands an
; explicit name: (E0104) instead of failing to find the subject.
(function_definition name: (identifier) @subject.name) @subject.function
(class_definition name: (identifier) @subject.name) @subject.type
(assignment left: (identifier) @subject.name) @subject.value
(assignment) @subject.value
