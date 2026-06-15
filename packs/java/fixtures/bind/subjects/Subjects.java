// @lore
// purpose: "a class subject (@subject.type)"
class Subjects {
  // @lore
  // kind: state
  // name: gamma
  int gamma = 0;

  // @lore
  // purpose: "a method subject (@subject.function)"
  void alpha() {}

  // @lore
  // purpose: "annotated method — exercises the marker_annotation skip (D-050c)"
  @Override
  void beta() {}
}

// @lore
// purpose: "an interface subject (@subject.type)"
interface Helper {}
