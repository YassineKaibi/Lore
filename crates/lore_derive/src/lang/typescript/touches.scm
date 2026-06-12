; §8.3 occurrence scan sites: bare identifiers (own-module / named-import
; forms) and alias.prop accesses (namespace-import form, D-062d).
(identifier) @id
(member_expression
  object: (identifier) @obj
  property: (property_identifier) @attr) @access
