// A reactive object for a component's props in a test: changing a field
// updates the mounted component, as a parent's state would.
export function reactive<T extends object>(value: T): T {
  const s = $state(value);
  return s;
}
