export function toCandidOpt<T>(value?: T | null): [] | [T] {
  if (value === undefined || value === null) {
    return [];
  }

  return [value];
}

export function fromCandidOpt<T>(value: [] | [T]): T | null {
  if (value.length === 0) {
    return null;
  }

  return value[0];
}
