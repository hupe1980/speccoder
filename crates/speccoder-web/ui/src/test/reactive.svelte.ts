// Test-only: a stand-in made reactive, for a component whose effects
// follow what changes on it (an app's job ending).
export function reactive<T extends object>(value: T): T {
  const s = $state(value);
  return s;
}
