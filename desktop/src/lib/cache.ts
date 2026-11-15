// Answers the screens show (app info, folder size, storage, displays, mics), kept while the app
// runs so a screen shows the last answer at once and refreshes it quietly. Cleared on logout:
// the next account mustn't see this one's numbers.

const values = new Map<string, unknown>();
const running = new Map<string, Promise<unknown>>();
/** Bumped by `forgetCache`, so a load still running then doesn't store its answer. */
let generation = 0;

/** The last value stored under `key`, if any. */
export function cached<T>(key: string): T | undefined {
  return values.get(key) as T | undefined;
}

/** Loads `key` again and stores the answer; calls made while a load runs share it. */
export function refresh<T>(key: string, load: () => Promise<T>): Promise<T> {
  const inFlight = running.get(key);
  if (inFlight) return inFlight as Promise<T>;
  const mine = generation;
  const promise = load()
    .then((value) => {
      if (mine === generation) values.set(key, value);
      return value;
    })
    .finally(() => {
      if (running.get(key) === promise) running.delete(key);
    });
  running.set(key, promise);
  return promise;
}

/** Forgets everything, e.g. on logout. */
export function forgetCache() {
  generation++;
  values.clear();
  running.clear();
}
