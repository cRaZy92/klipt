import { hash, verify } from '@node-rs/argon2';

// Algorithm.Argon2id === 2 (const enum in the package; use the literal under isolatedModules).
const OPTIONS = { algorithm: 2, memoryCost: 19456, timeCost: 2, parallelism: 1 } as const;

export function hashPassword(pw: string): Promise<string> {
  return hash(pw, OPTIONS);
}

export async function verifyPassword(hashed: string, pw: string): Promise<boolean> {
  try {
    return await verify(hashed, pw);
  } catch {
    return false;
  }
}
