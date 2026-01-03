/** @dose
purpose: Sample TSX fixture for testing React component and hook extraction.
    This demonstrates JSX syntax handling and React-specific patterns.

when-editing:
    - !Maintain hook usage patterns for hook detection testing
    - Keep both function and arrow component styles

invariants:
    - Components must use proper React patterns
    - Hooks must follow rules of hooks

gotchas:
    - TSX requires special handling for generic arrow functions
    - Hook detection relies on use* naming convention
*/

import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { UserConfig, UserService } from './sample';
import type { ReactNode, FC } from 'react';

// Type exports
export type ButtonVariant = 'primary' | 'secondary' | 'danger';

export interface ButtonProps {
    variant?: ButtonVariant;
    children: ReactNode;
    onClick?: () => void;
    disabled?: boolean;
}

// Custom hook export
export function useUser(userId: string) {
    const [user, setUser] = useState<UserConfig | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<Error | null>(null);

    useEffect(() => {
        const service = new UserService({ id: '', name: '', settings: {} });
        service.getUser(userId)
            .then(setUser)
            .catch(setError)
            .finally(() => setLoading(false));
    }, [userId]);

    return { user, loading, error };
}

// Another custom hook
export const useToggle = (initial = false) => {
    const [value, setValue] = useState(initial);
    const toggle = useCallback(() => setValue(v => !v), []);
    return [value, toggle] as const;
};

// Function component export
export function Button({ variant = 'primary', children, onClick, disabled }: ButtonProps) {
    const className = useMemo(
        () => `btn btn-${variant}`,
        [variant]
    );

    return (
        <button className={className} onClick={onClick} disabled={disabled}>
            {children}
        </button>
    );
}

// Arrow function component
export const UserCard: FC<{ user: UserConfig }> = ({ user }) => {
    const [expanded, toggleExpanded] = useToggle();

    return (
        <div className="user-card">
            <h3>{user.name}</h3>
            <p>ID: {user.id}</p>
            {expanded && (
                <pre>{JSON.stringify(user.settings, null, 2)}</pre>
            )}
            <Button onClick={toggleExpanded}>
                {expanded ? 'Collapse' : 'Expand'}
            </Button>
        </div>
    );
};

// Component with generics
export function List<T>({ items, renderItem }: { items: T[]; renderItem: (item: T) => ReactNode }) {
    return (
        <ul>
            {items.map((item, i) => (
                <li key={i}>{renderItem(item)}</li>
            ))}
        </ul>
    );
}

// Default export
export default UserCard;
