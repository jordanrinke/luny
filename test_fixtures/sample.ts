/** @toon
purpose: Sample TypeScript fixture for testing the luny parser.
    This file contains various TypeScript constructs to verify extraction works correctly.

when-editing:
    - !Keep all export types represented for comprehensive testing
    - Maintain the mix of sync and async functions

invariants:
    - All exported items must have clear, testable names
    - Import statements should cover all supported patterns

do-not:
    - Remove any exports without updating corresponding tests
*/

import { readFile, writeFile } from 'fs/promises';
import * as path from 'path';
import defaultExport from './other-module';
import type { SomeType } from './types';

// Type exports
export type UserId = string;
export type Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };

// Interface export
export interface UserConfig {
    id: UserId;
    name: string;
    settings: Record<string, unknown>;
}

// Enum export
export enum Status {
    Active = 'active',
    Inactive = 'inactive',
    Pending = 'pending',
}

// Class export
export class UserService {
    private cache: Map<string, UserConfig> = new Map();

    constructor(private readonly config: UserConfig) {}

    async getUser(id: UserId): Promise<UserConfig | null> {
        if (this.cache.has(id)) {
            return this.cache.get(id) || null;
        }
        const data = await readFile(path.join('users', id + '.json'), 'utf-8');
        return JSON.parse(data);
    }

    setUser(user: UserConfig): void {
        this.cache.set(user.id, user);
    }
}

// Function exports
export function validateUser(user: UserConfig): boolean {
    return user.id.length > 0 && user.name.length > 0;
}

export async function saveUser(user: UserConfig): Promise<void> {
    const filePath = path.join('users', user.id + '.json');
    await writeFile(filePath, JSON.stringify(user, null, 2));
}

// Arrow function export
export const createUser = (name: string): UserConfig => ({
    id: crypto.randomUUID(),
    name,
    settings: {},
});

// Const export
export const DEFAULT_CONFIG: UserConfig = {
    id: 'default',
    name: 'Default User',
    settings: {},
};

// Re-export
export { defaultExport };

// Default export
export default UserService;
