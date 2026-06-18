// @veridikt
// purpose: "a class subject (@subject.type)"
class Subjects {
  // @veridikt
  // kind: state
  // name: gamma
  int gamma = 0;

  // @veridikt
  // purpose: "a method subject (@subject.function)"
  void alpha() {}

  // @veridikt
  // purpose: "annotated method — exercises the marker_annotation skip (D-050c)"
  @Override
  void beta() {}
}

// @veridikt
// purpose: "an interface subject (@subject.type)"
interface Helper {}
